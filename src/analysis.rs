use alcaparra::{Interpreter, LintDiagnostic, Severity};
use alcaparra::lexer::{self, Token};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::project_config::ProjectConfig;
use crate::scope;

/// Analiza el texto fuente y devuelve diagnósticos LSP.
///
/// Ejecuta tres pasadas:
/// - `Interpreter::validate` → errores de lex/parse
/// - `Interpreter::lint`     → advertencias estáticas (MISSING_EMIT, DEAD_EMIT, etc.)
/// - `scope::check_undefined`→ variables/funciones no declaradas
pub fn analyze(source: &str, config: Option<&ProjectConfig>) -> Vec<Diagnostic> {
    let interp = match config {
        Some(cfg) => Interpreter::with_loader(cfg.root.clone(), cfg.aliases.clone()),
        None      => Interpreter::new(),
    };

    // Pasada 1: errores de lex/parse
    let mut diagnostics: Vec<Diagnostic> = match interp.validate(source) {
        Ok(warnings) => warnings.iter().map(validate_warning_to_diagnostic).collect(),
        Err(errors)  => return errors.iter().map(error_to_diagnostic).collect(),
    };

    // Pasada 2: lint estático
    // Los archivos sin bloque `main` son librerías importables — no necesitan `emit`.
    let is_library = !has_main_block(source);
    let lint_diags = Interpreter::lint(source);
    diagnostics.extend(
        lint_diags
            .iter()
            .filter(|d| !(is_library && d.code == "MISSING_EMIT"))
            .map(lint_to_diagnostic),
    );

    // Pasada 3: análisis de scope (variables/funciones indefinidas + redeclaraciones + imports no usados)
    let context_keys = config.map(|c| c.context_keys.as_slice()).unwrap_or(&[]);
    diagnostics.extend(scope::check_undefined(source, context_keys, config));
    diagnostics.extend(scope::check_unused_imports(source));

    // Pasada 4: validación de módulos/aliases en sentencias `use`
    diagnostics.extend(check_invalid_imports(source, config));

    diagnostics
}

/// Valida que cada sentencia `use` referencie un módulo stdlib o alias conocido.
/// - `use std.xxx`   → verifica que `xxx` sea un módulo stdlib válido
/// - `use @alias.xx` → verifica que `@alias` esté definido en `.capercfg`
/// - cualquier otro  → error (módulo desconocido)
pub fn check_invalid_imports(source: &str, config: Option<&ProjectConfig>) -> Vec<Diagnostic> {
    use alcaparra::parser::ast::Stmt;

    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };
    let program = match alcaparra::parser::parse(tokens) {
        Ok(p)  => p,
        Err(_) => return vec![],
    };

    let mut diags = Vec::new();

    for stmt in &program.body {
        let Stmt::Use { path, line, .. } = stmt else { continue };
        let first = match path.first() { Some(s) => s.as_str(), None => continue };

        if first == "std" {
            let module = path.get(1).map(|s| s.as_str()).unwrap_or("");
            if !module.is_empty() && !is_valid_stdlib_module(module) {
                let col = col_of_token(source, *line, module);
                diags.push(import_diagnostic(
                    *line, col, module.len(),
                    format!("módulo stdlib desconocido: `std.{module}`"),
                ));
            }
        } else if first.starts_with('@') {
            let alias_known = config
                .map(|c| c.aliases.contains_key(first))
                .unwrap_or(false);
            if !alias_known {
                let col = col_of_token(source, *line, first);
                diags.push(import_diagnostic(
                    *line, col, first.len(),
                    format!("alias `{first}` no definido en .capercfg"),
                ));
            }
        } else {
            // No es `std` ni `@alias` → módulo completamente desconocido
            let col = col_of_token(source, *line, first);
            diags.push(import_diagnostic(
                *line, col, first.len(),
                format!("módulo desconocido: `{first}` — usa `std.<modulo>` o `@alias.<archivo>`"),
            ));
        }
    }

    diags
}

fn is_valid_stdlib_module(module: &str) -> bool {
    alcaparra::stdlib::MODULES.iter().any(|m| m.name == module)
}

fn col_of_token(source: &str, line: usize, token: &str) -> u32 {
    let line_text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let byte_col  = line_text.find(token).unwrap_or(0);
    line_text[..byte_col].chars().map(|c| c.len_utf16() as u32).sum()
}

fn import_diagnostic(line: usize, col: u32, len: usize, message: String) -> Diagnostic {
    let l = line.saturating_sub(1) as u32;
    Diagnostic {
        range: Range {
            start: Position { line: l, character: col },
            end:   Position { line: l, character: col + len as u32 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code:     Some(NumberOrString::String("INVALID_IMPORT".to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message,
        ..Default::default()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Devuelve `true` si el fuente contiene un bloque `main { }`.
/// Los archivos sin `main` son librerías importables.
fn has_main_block(source: &str) -> bool {
    lexer::tokenize(source)
        .map(|tokens| tokens.iter().any(|s| matches!(s.token, Token::Main)))
        .unwrap_or(false)
}

// ── Conversores ─────────────────────────────────────────────────────────────

fn error_to_diagnostic(err: &alcaparra::CaperError) -> Diagnostic {
    Diagnostic {
        range:    position_to_range(err.position()),
        severity: Some(DiagnosticSeverity::ERROR),
        code:     None,
        source:   Some("alcaparra-lsp".to_string()),
        message:  err.to_string(),
        ..Default::default()
    }
}

fn validate_warning_to_diagnostic(warn: &alcaparra::Warning) -> Diagnostic {
    Diagnostic {
        range:    position_to_range(Some((warn.line, 0))),
        severity: Some(DiagnosticSeverity::WARNING),
        code:     Some(NumberOrString::String(warn.code.to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message:  warn.detail.clone(),
        ..Default::default()
    }
}

fn lint_to_diagnostic(d: &LintDiagnostic) -> Diagnostic {
    let severity = match d.severity {
        Severity::Error   => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };
    Diagnostic {
        range:    position_to_range(Some((d.line, d.col))),
        severity: Some(severity),
        code:     Some(NumberOrString::String(d.code.to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message:  d.message.clone(),
        ..Default::default()
    }
}

fn position_to_range(pos: Option<(usize, usize)>) -> Range {
    match pos {
        Some((line, col)) => {
            let l = line.saturating_sub(1) as u32;
            let c = col.saturating_sub(1) as u32;
            Range {
                start: Position { line: l, character: c },
                end:   Position { line: l, character: c + 1 },
            }
        }
        None => Range {
            start: Position { line: 0, character: 0 },
            end:   Position { line: 0, character: 1 },
        },
    }
}
