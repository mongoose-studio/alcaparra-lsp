use alcaparra::lexer::{self, Token};
use alcaparra::parser::{self, ast::{Stmt, UseItems}};
use tower_lsp::lsp_types::{
    DocumentSymbol, Location, Position, Range, SymbolKind, TextEdit, Url,
};

use crate::project_config::ProjectConfig;

/// Resultado de buscar una función (local o importada).
pub struct FnLocation {
    /// Nombres de los parámetros.
    pub params: Vec<String>,
    /// Código fuente del archivo donde fue encontrada (puede ser el mismo o uno externo).
    pub source: String,
}

/// Símbolo extraído del AST de un documento.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name:   String,
    pub kind:   SymbolKind,
    pub line:   usize,           // 1-based
    pub detail: Option<String>,  // Ej: parámetros de una función
}

impl Symbol {
    fn range(&self) -> Range {
        let l = self.line.saturating_sub(1) as u32;
        Range {
            start: Position { line: l, character: 0 },
            end:   Position { line: l, character: self.name.len() as u32 },
        }
    }

    /// Rango preciso usando la columna real del token (para goto-definition).
    fn precise_range(&self, col_1based: u32) -> Range {
        let l   = self.line.saturating_sub(1) as u32;
        let col = col_1based.saturating_sub(1);
        Range {
            start: Position { line: l, character: col },
            end:   Position { line: l, character: col + self.name.len() as u32 },
        }
    }

    pub fn to_document_symbol(&self) -> DocumentSymbol {
        #[allow(deprecated)]
        DocumentSymbol {
            name:            self.name.clone(),
            detail:          self.detail.clone(),
            kind:            self.kind,
            range:           self.range(),
            selection_range: self.range(),
            children:        None,
            tags:            None,
            deprecated:      None,
        }
    }

    pub fn to_location(&self, uri: &Url) -> Location {
        Location { uri: uri.clone(), range: self.range() }
    }
}

/// Busca en el token stream la columna (1-based) donde aparece una declaración de `name`.
/// Detecta `fn name`, `let name`, `var name`, `const name`.
fn decl_col(source: &str, name: &str) -> Option<u32> {
    let tokens = lexer::tokenize(source).ok()?;
    let mut expect_name = false;
    for tok in &tokens {
        match &tok.token {
            Token::Fn | Token::Let | Token::Var | Token::Const => {
                expect_name = true;
            }
            Token::Ident(n) if expect_name && n == name => {
                return Some(tok.col as u32);
            }
            _ => {
                expect_name = false;
            }
        }
    }
    None
}

/// Parsea el fuente y extrae todos los símbolos declarados:
/// funciones, variables, constantes y bloques header/main.
pub fn extract(source: &str) -> Vec<Symbol> {
    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };
    let program = match parser::parse(tokens) {
        Ok(p)  => p,
        Err(_) => return vec![],
    };

    let mut symbols = Vec::new();

    // Bloque header → constantes del script
    if let Some(header) = &program.header {
        for c in &header.constants {
            symbols.push(Symbol {
                name:   c.name.clone(),
                kind:   SymbolKind::CONSTANT,
                line:   c.line,
                detail: Some("const (header)".to_string()),
            });
        }
    }

    // Cuerpo principal
    extract_stmts(&program.body, &mut symbols);

    symbols
}

fn extract_stmts(stmts: &[Stmt], out: &mut Vec<Symbol>) {
    for stmt in stmts {
        match stmt {
            Stmt::FnDecl { name, params, line, .. } => {
                out.push(Symbol {
                    name:   name.clone(),
                    kind:   SymbolKind::FUNCTION,
                    line:   *line,
                    detail: Some(format!("fn {}({})", name, params.join(", "))),
                });
            }
            Stmt::Let { name, line, .. } => {
                out.push(Symbol {
                    name:   name.clone(),
                    kind:   SymbolKind::VARIABLE,
                    line:   *line,
                    detail: Some("let".to_string()),
                });
            }
            Stmt::Var { name, line, .. } => {
                out.push(Symbol {
                    name:   name.clone(),
                    kind:   SymbolKind::VARIABLE,
                    line:   *line,
                    detail: Some("var".to_string()),
                });
            }
            // Buscar símbolos dentro de bloques anidados
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                extract_stmts(then_block, out);
                for (_, block) in else_if_clauses {
                    extract_stmts(block, out);
                }
                if let Some(block) = else_block {
                    extract_stmts(block, out);
                }
            }
            Stmt::While { body, .. } => extract_stmts(body, out),
            Stmt::Foreach { body, .. } => extract_stmts(body, out),
            Stmt::TryCatch { try_block, catch_block, .. } => {
                extract_stmts(try_block, out);
                extract_stmts(catch_block, out);
            }
            _ => {}
        }
    }
}

/// Retorna los nombres de parámetros de una función declarada en el script, si existe.
/// Usa el AST primero; si falla (ej. función dentro de `main {}`), cae a búsqueda textual.
pub fn find_fn_params(source: &str, name: &str) -> Option<Vec<String>> {
    // AST-based (cubre la mayoría de los casos)
    if let Some(params) = (|| {
        let tokens  = lexer::tokenize(source).ok()?;
        let program = parser::parse(tokens).ok()?;
        find_fn_params_in_stmts(&program.body, name)
    })() {
        return Some(params);
    }
    // Fallback textual — cubre funciones dentro de main {} u otros bloques
    find_fn_params_text(source, name)
}

/// Busca una función en el archivo actual Y en archivos externos importados.
/// Retorna dónde fue encontrada para que el caller pueda buscar el DocBlock en la fuente correcta.
pub fn find_fn_anywhere(
    source: &str,
    name:   &str,
    config: Option<&ProjectConfig>,
) -> Option<FnLocation> {
    // 1. Local
    if let Some(params) = find_fn_params(source, name) {
        return Some(FnLocation { params, source: source.to_string() });
    }

    // 2. Archivos externos importados
    let config  = config?;
    let tokens  = lexer::tokenize(source).ok()?;
    let program = parser::parse(tokens).ok()?;

    for stmt in &program.body {
        let Stmt::Use { path, items, .. } = stmt else { continue };

        let could_import = match items {
            UseItems::Named(fns) => fns.iter().any(|f| f == name),
            UseItems::Single     => path.last().map(|l| l == name).unwrap_or(false),
            UseItems::All        => true,
        };
        if !could_import { continue; }

        let file_parts: &[String] = match items {
            UseItems::Single if path.len() > 1 => &path[..path.len() - 1],
            _ => path,
        };

        if let Some(file_path) = config.resolve_use_path(file_parts) {
            if let Ok(ext_source) = std::fs::read_to_string(&file_path) {
                if let Some(params) = find_fn_params(&ext_source, name) {
                    return Some(FnLocation { params, source: ext_source });
                }
            }
        }
    }

    None
}

// ── Búsqueda textual (fallback para funciones dentro de bloques) ──────────────

/// Extrae los parámetros de `fn name(...)` escaneando el texto línea a línea.
fn find_fn_params_text(source: &str, name: &str) -> Option<Vec<String>> {
    let pattern = format!("fn {}(", name);
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(after_paren) = trimmed.strip_prefix(&pattern) else { continue };
        let close  = after_paren.find(')')?;
        let params_str = &after_paren[..close];
        let params = if params_str.trim().is_empty() {
            vec![]
        } else {
            params_str.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
        };
        return Some(params);
    }
    None
}

/// Retorna la línea (1-based) donde está declarada `fn name`, escaneando el texto.
/// Exportada para uso en call hierarchy.
pub fn find_fn_line(source: &str, name: &str) -> Option<usize> {
    find_fn_line_text(source, name)
}

fn find_fn_line_text(source: &str, name: &str) -> Option<usize> {
    let pattern = format!("fn {}(", name);
    for (i, line) in source.lines().enumerate() {
        if line.trim().starts_with(&pattern) {
            return Some(i + 1);
        }
    }
    None
}

fn find_fn_params_in_stmts(stmts: &[Stmt], name: &str) -> Option<Vec<String>> {
    for stmt in stmts {
        match stmt {
            Stmt::FnDecl { name: n, params, body, .. } => {
                if n == name {
                    return Some(params.clone());
                }
                // Buscar en funciones anidadas
                if let Some(found) = find_fn_params_in_stmts(body, name) {
                    return Some(found);
                }
            }
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                if let Some(f) = find_fn_params_in_stmts(then_block, name) { return Some(f); }
                for (_, b) in else_if_clauses {
                    if let Some(f) = find_fn_params_in_stmts(b, name) { return Some(f); }
                }
                if let Some(b) = else_block {
                    if let Some(f) = find_fn_params_in_stmts(b, name) { return Some(f); }
                }
            }
            Stmt::While { body, .. } | Stmt::Foreach { body, .. } => {
                if let Some(f) = find_fn_params_in_stmts(body, name) { return Some(f); }
            }
            Stmt::TryCatch { try_block, catch_block, .. } => {
                if let Some(f) = find_fn_params_in_stmts(try_block, name) { return Some(f); }
                if let Some(f) = find_fn_params_in_stmts(catch_block, name) { return Some(f); }
            }
            _ => {}
        }
    }
    None
}

/// Busca la declaración de `name`, primero en el archivo actual y luego en
/// los archivos externos importados mediante `use`.
pub fn find_definition(
    source: &str,
    uri:    &Url,
    name:   &str,
    config: Option<&ProjectConfig>,
) -> Option<Location> {
    // 1. AST-based en el archivo actual
    if let Some(sym) = extract(source).into_iter().find(|s| s.name == name) {
        let col = decl_col(source, name).unwrap_or(1);
        let range = sym.precise_range(col);
        return Some(Location { uri: uri.clone(), range });
    }

    // 1b. Fallback textual — cubre `fn` dentro de `main {}` u otros bloques
    if let Some(line_1based) = find_fn_line_text(source, name) {
        let l   = (line_1based - 1) as u32;
        let col = decl_col(source, name).map(|c| c.saturating_sub(1)).unwrap_or(0);
        let range = Range {
            start: Position { line: l, character: col },
            end:   Position { line: l, character: col + name.len() as u32 },
        };
        return Some(Location { uri: uri.clone(), range });
    }

    // 2. Buscar en archivos externos importados
    let config = config?;
    let tokens  = lexer::tokenize(source).ok()?;
    let program = parser::parse(tokens).ok()?;

    for stmt in &program.body {
        let Stmt::Use { path, items, .. } = stmt else { continue };

        // Determinar si `name` podría venir de este `use`
        let could_import = match items {
            UseItems::Named(fns) => fns.iter().any(|f| f == name),
            UseItems::Single     => path.last().map(|l| l == name).unwrap_or(false),
            UseItems::All        => true,
        };
        if !could_import { continue; }

        // Para UseItems::Single el nombre de función es el último segmento:
        // `use @scripts.calculos.calcular` → file = @scripts/calculos.caper
        let file_path_parts: &[String] = match items {
            UseItems::Single if path.len() > 1 => &path[..path.len() - 1],
            _ => path,
        };

        if let Some(file_path) = config.resolve_use_path(file_path_parts) {
            if let Ok(ext_source) = std::fs::read_to_string(&file_path) {
                if let Some(sym) = extract(&ext_source).into_iter().find(|s| s.name == name) {
                    if let Ok(ext_uri) = Url::from_file_path(&file_path) {
                        let col = decl_col(&ext_source, name).unwrap_or(1);
                        let range = sym.precise_range(col);
                        return Some(Location { uri: ext_uri, range });
                    }
                }
            }
        }
    }

    // 3. Variable de contexto del .capercfg → navegar a la clave en el archivo
    if config.context_keys.iter().any(|k| k == name) {
        if let Some(loc) = find_in_capercfg(&config.root, name) {
            return Some(loc);
        }
    }

    None
}

/// Busca la clave `name` dentro del objeto `context` del `.capercfg` y retorna
/// su ubicación (línea exacta dentro del archivo JSON).
fn find_in_capercfg(root: &std::path::Path, name: &str) -> Option<Location> {
    let cfg_path = root.join(".capercfg");
    let content  = std::fs::read_to_string(&cfg_path).ok()?;
    let cfg_uri  = Url::from_file_path(&cfg_path).ok()?;

    // Buscar la línea que contiene `"name"` como clave JSON (dentro del bloque context)
    let needle = format!("\"{}\"", name);
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&needle) {
            let col = line.find(&needle).unwrap_or(0) as u32;
            let range = Range {
                start: Position { line: i as u32, character: col },
                end:   Position { line: i as u32, character: col + needle.len() as u32 },
            };
            return Some(Location { uri: cfg_uri, range });
        }
    }

    // Fallback: primera línea del archivo si la clave no se encuentra textualmente
    Some(Location {
        uri: cfg_uri,
        range: Range {
            start: Position { line: 0, character: 0 },
            end:   Position { line: 0, character: 0 },
        },
    })
}

/// Retorna la ubicación de TODAS las ocurrencias de `name` como identificador.
/// Incluye tanto declaraciones como usos.
pub fn find_references(source: &str, uri: &Url, name: &str) -> Vec<Location> {
    collect_ident_locations(source, uri, name, None)
}

/// Como `find_references`, pero limitado al bloque `fn` o `main {}` que
/// contiene `cursor_line` (0-based LSP). Úsalo para `document_highlight`
/// para evitar resaltar ocurrencias en otras funciones.
pub fn find_references_scoped(
    source:      &str,
    uri:         &Url,
    name:        &str,
    cursor_line: u32,
) -> Vec<Location> {
    let scope = fn_scope_bounds(source, cursor_line);
    collect_ident_locations(source, uri, name, scope)
}

/// Itera los tokens e recoge las `Location` de cada `Token::Ident(name)`.
/// Si `scope` es `Some((start, end))`, solo incluye tokens en ese rango de líneas.
fn collect_ident_locations(
    source: &str,
    uri:    &Url,
    name:   &str,
    scope:  Option<(u32, u32)>,
) -> Vec<Location> {
    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };

    tokens
        .into_iter()
        .filter_map(|s| {
            if let Token::Ident(n) = &s.token {
                if n == name {
                    let line = s.line.saturating_sub(1) as u32;
                    if let Some((start, end)) = scope {
                        if line < start || line > end {
                            return None;
                        }
                    }
                    let col = s.col.saturating_sub(1) as u32;
                    let range = Range {
                        start: Position { line, character: col },
                        end:   Position { line, character: col + name.len() as u32 },
                    };
                    return Some(Location { uri: uri.clone(), range });
                }
            }
            None
        })
        .collect()
}

/// Retorna el rango de líneas (0-based, inclusivo) del bloque `fn` o `main {}`
/// de nivel superior que contiene `cursor_line`. Retorna `None` si el cursor
/// está fuera de cualquier bloque (p.ej. en `use` o `header`).
fn fn_scope_bounds(source: &str, cursor_line: u32) -> Option<(u32, u32)> {
    let tokens = lexer::tokenize(source).ok()?;
    let mut scopes: Vec<(u32, u32)> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let tok = &tokens[i];

        // Solo detectamos `fn` y `main` a nivel global (depth 0)
        if matches!(tok.token, Token::Fn | Token::Main) {
            let scope_start = tok.line.saturating_sub(1) as u32;
            i += 1;

            // Avanzar hasta el primer `{` (omitiendo nombre y params) y rastrear depth
            let mut depth: i32 = 0;
            let mut found_open = false;

            while i < tokens.len() {
                match &tokens[i].token {
                    Token::LBrace => {
                        depth += 1;
                        found_open = true;
                    }
                    Token::RBrace => {
                        depth -= 1;
                        if found_open && depth == 0 {
                            let scope_end = tokens[i].line.saturating_sub(1) as u32;
                            scopes.push((scope_start, scope_end));
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        i += 1;
    }

    scopes.into_iter().find(|&(start, end)| cursor_line >= start && cursor_line <= end)
}

/// Genera los TextEdits necesarios para renombrar `old_name` → `new_name`
/// en todas sus ocurrencias dentro del documento.
pub fn rename_edits(source: &str, old_name: &str, new_name: &str) -> Vec<TextEdit> {
    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };

    tokens
        .into_iter()
        .filter_map(|s| {
            if let Token::Ident(n) = &s.token {
                if n == old_name {
                    let line = s.line.saturating_sub(1) as u32;
                    let col  = s.col.saturating_sub(1) as u32;
                    let range = Range {
                        start: Position { line, character: col },
                        end:   Position { line, character: col + old_name.len() as u32 },
                    };
                    return Some(TextEdit { range, new_text: new_name.to_string() });
                }
            }
            None
        })
        .collect()
}
