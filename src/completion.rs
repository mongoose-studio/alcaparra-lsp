use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    Position, Range, SymbolKind, TextEdit,
};

use alcaparra::lexer;
use alcaparra::parser::{self, ast::{Stmt, UseItems}};

use crate::catalog;
use crate::project_config::ProjectConfig;
use crate::symbols;
use crate::util;

// ── Contexto de completions ───────────────────────────────────────────────────

enum CompletionContext {
    /// Cursor dentro de `use std.` → sugerir nombres de módulo
    UseModule,
    /// Cursor dentro de `use std.math.{` → sugerir funciones de ese módulo
    UseFunctions { module: String },
    /// Cursor en línea `///` encima de un `fn` → ofrecer template de DocBlock
    DocBlockTemplate { fn_name: String, params: Vec<String> },
    /// Cualquier otro lugar → completions generales
    General,
}

/// Detecta el contexto del cursor mirando el texto de la línea actual.
fn detect_context(source: &str, position: Position) -> CompletionContext {
    let line = match source.lines().nth(position.line as usize) {
        Some(l) => l,
        None    => return CompletionContext::General,
    };
    let byte_off = util::utf16_to_byte_offset(line, position.character as usize).min(line.len());
    let prefix = &line[..byte_off];
    let trimmed = prefix.trim_start();

    // Contexto DocBlock: cursor en línea `///` con un `fn` en la línea siguiente
    if trimmed.starts_with("///") {
        if let Some(next_line) = source.lines().nth((position.line + 1) as usize) {
            if let Some((fn_name, params)) = parse_fn_header(next_line) {
                return CompletionContext::DocBlockTemplate { fn_name, params };
            }
        }
        return CompletionContext::General;
    }

    if !trimmed.starts_with("use ") && !trimmed.starts_with("use\t") {
        return CompletionContext::General;
    }

    let after_use = trimmed["use".len()..].trim_start();

    if !after_use.starts_with("std.") {
        return CompletionContext::General;
    }

    let after_std = &after_use["std.".len()..];

    // ¿Hay un punto después del nombre del módulo?
    if let Some(dot) = after_std.find('.') {
        let module = after_std[..dot].to_string();
        return CompletionContext::UseFunctions { module };
    }

    CompletionContext::UseModule
}

// ── Punto de entrada principal ────────────────────────────────────────────────

/// Construye la lista de completions apropiada para la posición actual.
pub fn completions(source: &str, position: Position, config: Option<&ProjectConfig>) -> Vec<CompletionItem> {
    match detect_context(source, position) {
        CompletionContext::UseModule                          => use_modules(),
        CompletionContext::UseFunctions { module }           => use_functions(&module),
        CompletionContext::DocBlockTemplate { fn_name, params } => docblock_template(&fn_name, &params),
        CompletionContext::General => {
            let mut items = Vec::new();
            items.extend(keywords());
            items.extend(snippets());
            items.extend(stdlib_fns(source));
            items.extend(local_symbols(source));
            items.extend(imported_fns(source, config));
            items.extend(unimported_external_fns(source, config));
            if let Some(cfg) = config {
                items.extend(context_vars(cfg));
            }
            items
        }
    }
}

// ── Completions para `use` ────────────────────────────────────────────────────

/// Sugiere los módulos de stdlib disponibles (`use std.<módulo>`).
fn use_modules() -> Vec<CompletionItem> {
    alcaparra::stdlib::MODULES
        .iter()
        .map(|m| CompletionItem {
            label:  m.name.to_string(),
            kind:   Some(CompletionItemKind::MODULE),
            detail: Some(format!("{} — {}", m.path, m.description)),
            insert_text: Some(format!("{}.{{ ${{1}} }};", m.name)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

/// Sugiere las funciones de un módulo específico (`use std.math.{ <fn> }`).
fn use_functions(module_name: &str) -> Vec<CompletionItem> {
    let Some(module) = alcaparra::stdlib::MODULES
        .iter()
        .find(|m| m.name == module_name)
    else {
        return vec![];
    };

    module
        .functions
        .iter()
        .map(|&name| {
            let (detail, doc) = match catalog::lookup(name) {
                Some(e) => (
                    Some(e.signature.to_string()),
                    Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!("```caper\n{}\n```\n\n{}", e.signature, e.doc),
                    })),
                ),
                None => (None, None),
            };
            CompletionItem {
                label: name.to_string(),
                kind:  Some(CompletionItemKind::FUNCTION),
                detail,
                documentation: doc,
                ..Default::default()
            }
        })
        .collect()
}

// ── Auto-import: helpers para code actions ────────────────────────────────────

/// Busca el módulo al que pertenece una función stdlib, si existe.
pub fn find_fn_module(fn_name: &str) -> Option<&'static str> {
    catalog::lookup(fn_name).map(|e| e.module)
}

/// Devuelve true si `fn_name` ya aparece importada en el fuente.
pub fn is_already_imported(source: &str, fn_name: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim();
        t.starts_with("use ") && t.contains(fn_name)
    })
}

/// Si el fuente ya tiene un bloque `use <module>.{...}`, devuelve un `(Range, String)`
/// para fusionar `fn_name` en ese bloque en lugar de insertar una nueva línea.
/// Devuelve `None` si no hay bloque existente para ese módulo.
pub fn make_merge_edit(
    source:  &str,
    module:  &str,
    fn_name: &str,
) -> Option<(tower_lsp::lsp_types::Range, String)> {
    use tower_lsp::lsp_types::{Position, Range};

    let use_prefix = format!("use {}.", module);
    let lines: Vec<&str> = source.lines().collect();

    // Línea donde abre el bloque (ej. "use std.math.{")
    let start = lines.iter().position(|l| l.trim_start().starts_with(&use_prefix))?;

    // Línea donde cierra el bloque (primera que contiene '}' desde start)
    let end = (start..lines.len()).find(|&i| lines[i].contains('}'))?;

    // Extraer nombres existentes de entre { ... }
    let block = lines[start..=end].join("\n");
    let open  = block.find('{')? + 1;
    let close = block.rfind('}')?;
    let inner = &block[open..close];

    let mut names: Vec<String> = inner
        .split([',', '\n'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if names.iter().any(|n| n == fn_name) {
        return None; // ya está importado, no duplicar
    }
    names.push(fn_name.to_string());

    // Indentación de la línea use y de los ítems internos
    let line_indent: String = lines[start].chars().take_while(|c| c.is_whitespace()).collect();
    let item_indent: String = if end > start {
        lines[start + 1].chars().take_while(|c| c.is_whitespace()).collect()
    } else {
        format!("{}    ", line_indent)
    };

    // Encabezado hasta el '{' inclusive: ej. "use std.math.{"
    let brace_pos  = lines[start].find('{').unwrap_or(lines[start].len());
    let use_header = &lines[start][..=brace_pos];

    // Reconstruir siempre en formato multilínea
    let items_text = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            if i + 1 < names.len() {
                format!("{}{},", item_indent, n)
            } else {
                format!("{}{}", item_indent, n)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let new_block = format!("{}\n{}\n{}}};", use_header, items_text, line_indent);

    let end_char: u32 = lines[end].chars().map(|c| c.len_utf16() as u32).sum();
    let range = Range {
        start: Position { line: start as u32, character: 0 },
        end:   Position { line: end   as u32, character: end_char },
    };

    Some((range, new_block))
}

/// Línea donde insertar un nuevo `use` (después del último `use` existente,
/// incluyendo bloques multilínea, o al inicio si no hay ninguno).
pub fn import_insert_line(source: &str) -> u32 {
    let lines: Vec<&str> = source.lines().collect();
    let mut last_end: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("use ") {
            // Bloque multilínea: `use foo.{` sin `}` en la misma línea
            if trimmed.contains('{') && !trimmed.contains('}') {
                if let Some(close) = (i + 1..lines.len()).find(|&j| lines[j].contains('}')) {
                    last_end = Some(close);
                    i = close + 1;
                    continue;
                }
            }
            last_end = Some(i);
        }
        i += 1;
    }
    last_end.map(|l| l as u32 + 1).unwrap_or(0)
}

/// Busca en los archivos externos (vía aliases del .capercfg) si `fn_name` está definida.
/// Retorna `(use_path, label)` donde `use_path` es el segmento a usar en `use <path>.{ fn };`
pub fn find_external_fn_import(
    fn_name: &str,
    config:  &ProjectConfig,
) -> Option<(String, String)> {
    for (alias, dir) in &config.aliases {
        if let Some(use_path) = find_fn_in_dir(fn_name, alias, dir, dir) {
            let label = format!("{}.{{ {} }}", use_path, fn_name);
            return Some((use_path, label));
        }
    }
    None
}

/// Completions de funciones en archivos externos que aún NO están importadas.
/// Cada item lleva `additional_text_edits` para insertar el `use` al aceptar.
fn unimported_external_fns(source: &str, config: Option<&ProjectConfig>) -> Vec<CompletionItem> {
    let config      = match config { Some(c) => c, None => return vec![] };
    let insert_line = import_insert_line(source);
    let mut items   = Vec::new();

    for (alias, dir) in &config.aliases {
        collect_external_fns(source, alias, dir, dir, insert_line, &mut items);
    }
    items
}

fn collect_external_fns(
    source:      &str,
    alias:       &str,
    base_dir:    &std::path::PathBuf,
    dir:         &std::path::PathBuf,
    insert_line: u32,
    items:       &mut Vec<CompletionItem>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_external_fns(source, alias, base_dir, &path, insert_line, items);
            continue;
        }
        if path.extension().map(|e| e != "caper").unwrap_or(true) { continue; }

        let Ok(ext_source) = std::fs::read_to_string(&path) else { continue };

        // Calcular use_path: alias + componentes relativas sin extensión
        let Some(rel)    = path.strip_prefix(base_dir).ok() else { continue };
        let rel_no_ext   = rel.with_extension("");
        let use_path: String = std::iter::once(alias)
            .chain(rel_no_ext.components().map(|c| c.as_os_str().to_str().unwrap_or("")))
            .collect::<Vec<_>>()
            .join(".");

        for sym in symbols::extract(&ext_source) {
            if sym.kind != SymbolKind::FUNCTION { continue; }
            let name = &sym.name;
            if is_already_imported(source, name) { continue; }

            let (range, new_text) =
                if let Some((mr, mt)) = make_merge_edit(source, &use_path, name) {
                    (mr, mt)
                } else {
                    (Range {
                        start: Position { line: insert_line, character: 0 },
                        end:   Position { line: insert_line, character: 0 },
                    }, format!("use {}.{{ {} }};\n", use_path, name))
                };

            items.push(CompletionItem {
                label:                name.clone(),
                kind:                 Some(CompletionItemKind::FUNCTION),
                detail:               Some(format!("{} — {}", use_path, name)),
                additional_text_edits: Some(vec![TextEdit { range, new_text }]),
                ..Default::default()
            });
        }
    }
}

fn find_fn_in_dir(
    fn_name:  &str,
    alias:    &str,
    base_dir: &std::path::PathBuf,
    dir:      &std::path::PathBuf,
) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(r) = find_fn_in_dir(fn_name, alias, base_dir, &path) {
                return Some(r);
            }
        } else if path.extension().map(|e| e == "caper").unwrap_or(false) {
            if let Ok(source) = std::fs::read_to_string(&path) {
                if crate::symbols::find_fn_params(&source, fn_name).is_some() {
                    let rel = path.strip_prefix(base_dir).ok()?;
                    let rel_no_ext = rel.with_extension("");
                    let segments: String = std::iter::once(alias.as_ref())
                        .chain(
                            rel_no_ext
                                .components()
                                .map(|c| c.as_os_str().to_str().unwrap_or(""))
                        )
                        .collect::<Vec<_>>()
                        .join(".");
                    return Some(segments);
                }
            }
        }
    }
    None
}

// ── Símbolos locales del documento ───────────────────────────────────────────

/// Completions de funciones, variables y constantes declaradas en el propio archivo.
/// Usa el AST cuando parsea correctamente; cae a escaneo textual si el fuente
/// está incompleto (como ocurre mientras el usuario escribe).
fn local_symbols(source: &str) -> Vec<CompletionItem> {
    let ast_syms = symbols::extract(source);
    if !ast_syms.is_empty() {
        return ast_syms
            .into_iter()
            .map(|sym| {
                let kind = match sym.kind {
                    SymbolKind::FUNCTION => CompletionItemKind::FUNCTION,
                    SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
                    _                   => CompletionItemKind::VARIABLE,
                };
                CompletionItem {
                    label:  sym.name,
                    kind:   Some(kind),
                    detail: sym.detail,
                    ..Default::default()
                }
            })
            .collect();
    }

    // Fallback textual — tolerante a fuente incompleto
    local_symbols_text(source)
}

/// Escanea el fuente línea a línea buscando declaraciones `fn`, `let`, `var` y `const`.
fn local_symbols_text(source: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut seen  = std::collections::HashSet::new();

    for line in source.lines() {
        let t = line.trim();

        // fn nombre(
        if let Some(rest) = t.strip_prefix("fn ") {
            if let Some(paren) = rest.find('(') {
                let name = rest[..paren].trim().to_string();
                if !name.is_empty() && seen.insert(name.clone()) {
                    let params_str = rest.get(paren + 1..)
                        .and_then(|s| s.find(')').map(|e| &s[..e]))
                        .unwrap_or("");
                    let detail = format!("fn {}({})", name, params_str);
                    items.push(CompletionItem {
                        label:  name,
                        kind:   Some(CompletionItemKind::FUNCTION),
                        detail: Some(detail),
                        ..Default::default()
                    });
                }
            }
            continue;
        }

        // let nombre  /  var nombre
        for prefix in &["let ", "var "] {
            if let Some(rest) = t.strip_prefix(prefix) {
                let name = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() && seen.insert(name.clone()) {
                    items.push(CompletionItem {
                        label:  name,
                        kind:   Some(CompletionItemKind::VARIABLE),
                        detail: Some(prefix.trim().to_string()),
                        ..Default::default()
                    });
                }
                break;
            }
        }

        // const NOMBRE = ...
        if let Some(rest) = t.strip_prefix("const ") {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("")
                .to_string();
            if !name.is_empty() && seen.insert(name.clone()) {
                items.push(CompletionItem {
                    label:  name,
                    kind:   Some(CompletionItemKind::CONSTANT),
                    detail: Some("const (header)".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    items
}

/// Completions de funciones importadas desde archivos externos (`use @alias.file.{ fn }`).
/// Intenta vía AST; si el fuente no parsea (escritura en curso), usa escaneo textual.
fn imported_fns(source: &str, config: Option<&ProjectConfig>) -> Vec<CompletionItem> {
    let config = match config { Some(c) => c, None => return vec![] };

    // ── Intento AST ──────────────────────────────────────────────────────────
    if let (Ok(tokens), ) = (lexer::tokenize(source), ) {
        if let Ok(program) = parser::parse(tokens) {
            let mut items = Vec::new();
            for stmt in &program.body {
                let Stmt::Use { path, items: use_items, .. } = stmt else { continue };
                let first = match path.first() { Some(s) => s, None => continue };
                if first == "std" { continue; }

                let file_parts: &[String] = match use_items {
                    UseItems::Single if path.len() > 1 => &path[..path.len() - 1],
                    _ => path,
                };
                let file_path = match config.resolve_use_path(file_parts) { Some(p) => p, None => continue };
                let ext_source = match std::fs::read_to_string(&file_path) { Ok(s) => s, Err(_) => continue };

                let names: Vec<String> = match use_items {
                    UseItems::Named(fns) => fns.clone(),
                    UseItems::Single     => path.last().map(|n| vec![n.clone()]).unwrap_or_default(),
                    UseItems::All        => symbols::extract(&ext_source)
                        .into_iter()
                        .filter(|s| s.kind == SymbolKind::FUNCTION)
                        .map(|s| s.name)
                        .collect(),
                };
                for name in names {
                    if let Some(params) = symbols::find_fn_params(&ext_source, &name) {
                        items.push(completion_for_fn(&name, &params));
                    }
                }
            }
            if !items.is_empty() { return items; }
        }
    }

    // ── Fallback textual ─────────────────────────────────────────────────────
    imported_fns_text(source, config)
}

/// Escanea las líneas `use @alias.file...` del fuente sin necesitar el AST.
fn imported_fns_text(source: &str, config: &ProjectConfig) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for line in source.lines() {
        let t = line.trim();
        let after_use = match t.strip_prefix("use ") { Some(s) => s.trim(), None => continue };

        // Solo imports externos (empieza con '@' o '.')
        if !after_use.starts_with('@') && !after_use.starts_with('.') { continue; }

        // Quitar ';' final si lo hay
        let after_use = after_use.trim_end_matches(';').trim();

        // Detectar `{ fn1, fn2 }` al final
        let (path_part, names): (&str, Vec<String>) = if let Some(brace) = after_use.find('{') {
            let path_raw = after_use[..brace].trim().trim_end_matches('.').trim();
            let inside   = after_use[brace + 1..].trim_end_matches('}').trim();
            let fns = inside.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            (path_raw, fns)
        } else {
            // `use @alias.file` (All) o `use @alias.file.fn` (Single)
            let segments: Vec<&str> = after_use.split('.').collect();
            if segments.len() < 2 { continue; }
            // Último segmento podría ser el nombre de función (Single) o el archivo (All)
            // Para el fallback asumimos All: el path completo es el archivo
            (after_use, vec![])
        };

        // Convertir path_part en Vec<String> de segmentos
        let path_segs: Vec<String> = path_part.split('.').map(|s| s.to_string()).collect();
        let file_path = match config.resolve_use_path(&path_segs) { Some(p) => p, None => continue };
        let ext_source = match std::fs::read_to_string(&file_path) { Ok(s) => s, Err(_) => continue };

        if names.is_empty() {
            // UseItems::All — todas las funciones del archivo
            for line in ext_source.lines() {
                let lt = line.trim();
                if let Some(rest) = lt.strip_prefix("fn ") {
                    if let Some(paren) = rest.find('(') {
                        let name = rest[..paren].trim().to_string();
                        let params_str = rest.get(paren + 1..)
                            .and_then(|s| s.find(')').map(|e| &s[..e]))
                            .unwrap_or("");
                        let params: Vec<String> = params_str.split(',')
                            .map(|p| p.trim().to_string())
                            .filter(|p| !p.is_empty())
                            .collect();
                        items.push(completion_for_fn(&name, &params));
                    }
                }
            }
        } else {
            for name in &names {
                if let Some(params) = symbols::find_fn_params(&ext_source, name) {
                    items.push(completion_for_fn(name, &params));
                }
            }
        }
    }

    items
}

fn completion_for_fn(name: &str, params: &[String]) -> CompletionItem {
    CompletionItem {
        label:  name.to_string(),
        kind:   Some(CompletionItemKind::FUNCTION),
        detail: Some(format!("fn {}({})", name, params.join(", "))),
        ..Default::default()
    }
}

// ── Keywords ─────────────────────────────────────────────────────────────────

fn keywords() -> Vec<CompletionItem> {
    const KWS: &[(&str, &str)] = &[
        ("let",      "Variable inmutable"),
        ("var",      "Variable mutable"),
        ("const",    "Constante de script (solo en header)"),
        ("fn",       "Declaración de función"),
        ("emit",     "Retorna un valor y termina la ejecución"),
        ("if",       "Condicional"),
        ("else",     "Rama alternativa del if"),
        ("match",    "Pattern matching"),
        ("while",    "Bucle con condición"),
        ("foreach",  "Itera sobre un array u objeto"),
        ("in",       "Parte de foreach"),
        ("break",    "Sale del bucle actual"),
        ("continue", "Salta a la siguiente iteración"),
        ("try",      "Bloque de manejo de errores"),
        ("catch",    "Captura excepciones del try"),
        ("throw",    "Lanza una excepción"),
        ("use",      "Importa funciones de la stdlib o de otro módulo"),
        ("header",   "Bloque de metadatos y constantes del script"),
        ("main",     "Bloque principal del script"),
        ("and",      "Operador lógico AND"),
        ("or",       "Operador lógico OR"),
        ("not",      "Operador lógico NOT"),
        ("true",     "Valor booleano verdadero"),
        ("false",    "Valor booleano falso"),
        ("null",     "Ausencia de valor"),
    ];
    KWS.iter()
        .map(|(kw, doc)| CompletionItem {
            label:  kw.to_string(),
            kind:   Some(CompletionItemKind::KEYWORD),
            detail: Some(doc.to_string()),
            ..Default::default()
        })
        .collect()
}

// ── Snippets ─────────────────────────────────────────────────────────────────

fn snippets() -> Vec<CompletionItem> {
    vec![
        snippet("fn",      "Función",           "fn ${1:nombre}(${2:params}) {\n\temit ${3:valor};\n}"),
        snippet("if",      "Condicional if/else","if ${1:condicion} {\n\t${2}\n} else {\n\t${3}\n}"),
        snippet("foreach", "Bucle foreach",      "foreach ${1:item} in ${2:items} {\n\t${3}\n}"),
        snippet("match",   "Pattern matching",   "match ${1:valor} {\n\t${2:patron} => ${3:resultado},\n\t_ => ${4:default}\n}"),
        snippet("try",     "try / catch",        "try {\n\t${1}\n} catch ${2:e} {\n\t${3}\n}"),
        snippet("header",  "Bloque header",      "header {\n\tname: \"${1:nombre}\";\n\tversion: \"${2:1.0.0}\";\n}"),
        snippet("main",    "Bloque main",        "main {\n\t${1}\n\temit { ${2} };\n}"),
        snippet("emit",    "emit objeto",        "emit { ${1:clave}: ${2:valor} };"),
        snippet("use",     "Importar de stdlib", "use std.${1:math}.{ ${2:round} };"),
        snippet("let",     "Variable inmutable", "let ${1:nombre} = ${2:valor};"),
        snippet("var",     "Variable mutable",   "var ${1:nombre} = ${2:valor};"),
    ]
}

fn snippet(label: &str, detail: &str, body: &str) -> CompletionItem {
    CompletionItem {
        label:              label.to_string(),
        kind:               Some(CompletionItemKind::SNIPPET),
        detail:             Some(detail.to_string()),
        insert_text:        Some(body.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

// ── Funciones stdlib (completions generales) ──────────────────────────────────

fn stdlib_fns(source: &str) -> Vec<CompletionItem> {
    let insert_line = import_insert_line(source);

    catalog::all_fn_names()
        .map(|name| {
            let (detail, doc) = match catalog::lookup(name) {
                Some(e) => (
                    Some(format!("{} — {}", e.module, e.signature)),
                    Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!("```caper\n{}\n```\n\n{}", e.signature, e.doc),
                    })),
                ),
                None => (None, None),
            };

            // Si la función no está importada, adjuntar el TextEdit de auto-import
            let additional_text_edits = if !is_already_imported(source, name) {
                if let Some(module) = catalog::lookup(name).map(|e| e.module) {
                    let (range, new_text) =
                        if let Some((merge_range, merged)) = make_merge_edit(source, module, name) {
                            (merge_range, merged)
                        } else {
                            let line = insert_line;
                            (Range {
                                start: Position { line, character: 0 },
                                end:   Position { line, character: 0 },
                            }, format!("use {}.{{ {} }};\n", module, name))
                        };
                    Some(vec![TextEdit { range, new_text }])
                } else {
                    None
                }
            } else {
                None
            };

            CompletionItem {
                label: name.to_string(),
                kind:  Some(CompletionItemKind::FUNCTION),
                detail,
                documentation: doc,
                additional_text_edits,
                ..Default::default()
            }
        })
        .collect()
}

// ── DocBlock template ─────────────────────────────────────────────────────────

/// Extrae `(fn_name, params)` de una línea tipo `fn foo(x, y) {`.
/// Retorna `None` si la línea no es una declaración de función.
fn parse_fn_header(line: &str) -> Option<(String, Vec<String>)> {
    let after_fn = line.trim().strip_prefix("fn ")?;
    let paren    = after_fn.find('(')?;
    let fn_name  = after_fn[..paren].trim().to_string();
    if fn_name.is_empty() { return None; }

    let close_paren = after_fn.find(')')?;
    let params_str  = &after_fn[paren + 1..close_paren];
    let params: Vec<String> = params_str
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    Some((fn_name, params))
}

/// Genera el item de completion con el template de DocBlock para `fn_name(params)`.
/// El cursor (`$0`) queda al final para que el usuario empiece a escribir la descripción.
fn docblock_template(fn_name: &str, params: &[String]) -> Vec<CompletionItem> {
    // La línea actual ya tiene `///` escrito; el texto del insert_text
    // reemplaza esa parte (el editor inserta desde el cursor, no desde el inicio de línea).
    // Usamos PLAIN_TEXT porque el contenido no tiene placeholders de snippet.
    let mut lines: Vec<String> = vec![format!(" {fn_name}.")];
    for p in params {
        lines.push(format!("\n/// @param {p:<12} "));
    }
    lines.push("\n/// @returns ".to_string());

    let insert_text = lines.concat();

    vec![CompletionItem {
        label:              format!("/// DocBlock: {fn_name}"),
        kind:               Some(CompletionItemKind::SNIPPET),
        detail:             Some("Inserta template de documentación".to_string()),
        insert_text:        Some(insert_text),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        preselect:          Some(true),
        sort_text:          Some("000".to_string()), // aparece primero
        ..Default::default()
    }]
}

// ── Variables de contexto (.capercfg) ────────────────────────────────────────

fn context_vars(config: &ProjectConfig) -> Vec<CompletionItem> {
    config
        .context_keys
        .iter()
        .map(|key| CompletionItem {
            label:  key.clone(),
            kind:   Some(CompletionItemKind::VARIABLE),
            detail: Some("variable de contexto (.capercfg)".to_string()),
            ..Default::default()
        })
        .collect()
}
