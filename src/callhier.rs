/// Call hierarchy: prepara el ítem, busca llamadas entrantes y salientes.
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams,
    CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams,
    Position, Range, SymbolKind, Url,
};

use alcaparra::lexer::{self, Token};
use alcaparra::parser::{self, ast::{Expr, Stmt}};

use crate::{hover, project_config::ProjectConfig, symbols, wsymbols};

// ── Prepare ───────────────────────────────────────────────────────────────────

/// Identifica el ítem de call hierarchy bajo el cursor.
/// Solo devuelve resultado si el cursor está sobre el nombre de una función declarada.
pub fn prepare(
    source:   &str,
    uri:      &Url,
    position: Position,
) -> Option<Vec<CallHierarchyItem>> {
    let word = hover::word_at_position(source, position)?;

    // Buscar la declaración de esa función en el archivo actual
    let sym = symbols::extract(source)
        .into_iter()
        .find(|s| s.name == word && s.kind == SymbolKind::FUNCTION)?;

    Some(vec![make_item(word.to_string(), uri.clone(), sym_range(&sym.name, sym.line))])
}

// ── Incoming calls ────────────────────────────────────────────────────────────

/// Busca todos los lugares del proyecto donde se llama a `fn_name`.
pub fn incoming_calls(
    params: CallHierarchyIncomingCallsParams,
    config: Option<&ProjectConfig>,
) -> Vec<CallHierarchyIncomingCall> {
    let fn_name = params.item.name.as_str();
    let root    = match config { Some(c) => c.root.clone(), None => return vec![] };

    let mut calls = Vec::new();

    for file_path in wsymbols::find_caper_files(&root) {
        let source = match std::fs::read_to_string(&file_path) { Ok(s) => s, Err(_) => continue };
        let uri    = match Url::from_file_path(&file_path) { Ok(u) => u, Err(_) => continue };

        let call_ranges = find_call_sites(&source, fn_name);
        if call_ranges.is_empty() { continue; }

        // El "from" es el archivo/módulo que contiene las llamadas (símbolo contenedor más cercano)
        let from_item = make_item(
            file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string(),
            uri,
            Range::default(),
        );

        calls.push(CallHierarchyIncomingCall {
            from:             from_item,
            from_ranges:      call_ranges,
        });
    }

    calls
}

// ── Outgoing calls ────────────────────────────────────────────────────────────

/// Lista todas las funciones que llama `fn_name` desde su cuerpo.
pub fn outgoing_calls(
    params: CallHierarchyOutgoingCallsParams,
    source: &str,
    config: Option<&ProjectConfig>,
) -> Vec<CallHierarchyOutgoingCall> {
    let fn_name = params.item.name.as_str();

    let tokens  = match lexer::tokenize(source)  { Ok(t) => t, Err(_) => return vec![] };
    let program = match parser::parse(tokens) { Ok(p) => p, Err(_) => return vec![] };

    // Buscar el cuerpo de la función
    let body = match find_fn_body(&program.body, fn_name) {
        Some(b) => b,
        None    => return vec![],
    };

    // Recoger todas las llamadas dentro del cuerpo
    let mut called: std::collections::HashMap<String, Vec<Range>> = std::collections::HashMap::new();
    collect_calls_in_stmts(body, &mut called);

    called
        .into_iter()
        .filter_map(|(name, ranges)| {
            // Buscar la definición del callee para construir el CallHierarchyItem
            let uri_and_line = find_callee_location(&name, source, config);
            let (uri, item_range) = uri_and_line?;
            Some(CallHierarchyOutgoingCall {
                to:          make_item(name, uri, item_range),
                from_ranges: ranges,
            })
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_item(name: String, uri: Url, range: Range) -> CallHierarchyItem {
    CallHierarchyItem {
        name,
        kind:            SymbolKind::FUNCTION,
        uri,
        range,
        selection_range: range,
        tags:            None,
        detail:          None,
        data:            None,
    }
}

fn sym_range(name: &str, line_1based: usize) -> Range {
    let l = line_1based.saturating_sub(1) as u32;
    Range {
        start: Position { line: l, character: 0 },
        end:   Position { line: l, character: name.len() as u32 },
    }
}

/// Encuentra los rangos en `source` donde se llama a `fn_name` (Ident seguido de LParen).
fn find_call_sites(source: &str, fn_name: &str) -> Vec<Range> {
    let tokens = match lexer::tokenize(source) { Ok(t) => t, Err(_) => return vec![] };
    let n = tokens.len();
    let mut ranges = Vec::new();

    for i in 0..n.saturating_sub(1) {
        if let Token::Ident(ref name) = tokens[i].token {
            if name == fn_name && tokens[i + 1].token == Token::LParen {
                let l   = tokens[i].line.saturating_sub(1) as u32;
                let col = tokens[i].col.saturating_sub(1) as u32;
                ranges.push(Range {
                    start: Position { line: l, character: col },
                    end:   Position { line: l, character: col + fn_name.len() as u32 },
                });
            }
        }
    }
    ranges
}

/// Recorre los stmts buscando el body de `fn name`.
fn find_fn_body<'a>(stmts: &'a [Stmt], name: &str) -> Option<&'a Vec<Stmt>> {
    for stmt in stmts {
        match stmt {
            Stmt::FnDecl { name: n, body, .. } => {
                if n == name { return Some(body); }
                if let Some(found) = find_fn_body(body, name) { return Some(found); }
            }
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                if let Some(f) = find_fn_body(then_block, name) { return Some(f); }
                for (_, b) in else_if_clauses { if let Some(f) = find_fn_body(b, name) { return Some(f); } }
                if let Some(b) = else_block { if let Some(f) = find_fn_body(b, name) { return Some(f); } }
            }
            Stmt::While { body, .. } | Stmt::Foreach { body, .. } => {
                if let Some(f) = find_fn_body(body, name) { return Some(f); }
            }
            Stmt::TryCatch { try_block, catch_block, .. } => {
                if let Some(f) = find_fn_body(try_block,  name) { return Some(f); }
                if let Some(f) = find_fn_body(catch_block, name) { return Some(f); }
            }
            _ => {}
        }
    }
    None
}

/// Recoge todas las llamadas a función (Expr::Call con callee Ident) dentro de un bloque.
fn collect_calls_in_stmts(
    stmts: &[Stmt],
    out:   &mut std::collections::HashMap<String, Vec<Range>>,
) {
    for stmt in stmts {
        let exprs: Vec<&Expr> = match stmt {
            Stmt::Let   { value, .. } | Stmt::Var  { value, .. } |
            Stmt::Emit  { value, .. } | Stmt::Throw { value, .. } |
            Stmt::Assign { value, .. } => vec![value],
            Stmt::ExprStmt(e) => vec![e],
            Stmt::If { condition, then_block, else_if_clauses, else_block, .. } => {
                collect_calls_in_stmts(then_block, out);
                for (_, b) in else_if_clauses { collect_calls_in_stmts(b, out); }
                if let Some(b) = else_block { collect_calls_in_stmts(b, out); }
                vec![condition]
            }
            Stmt::While   { condition, body, .. } => { collect_calls_in_stmts(body, out); vec![condition] }
            Stmt::Foreach { iter, body, .. }      => { collect_calls_in_stmts(body, out); vec![iter] }
            Stmt::FnDecl  { body, .. }            => { collect_calls_in_stmts(body, out); vec![] }
            Stmt::TryCatch { try_block, catch_block, .. } => {
                collect_calls_in_stmts(try_block,  out);
                collect_calls_in_stmts(catch_block, out);
                vec![]
            }
            _ => vec![],
        };
        for expr in exprs { collect_calls_in_expr(expr, out); }
    }
}

fn collect_calls_in_expr(
    expr: &Expr,
    out:  &mut std::collections::HashMap<String, Vec<Range>>,
) {
    match expr {
        Expr::Call { callee, args, line } => {
            if let Expr::Ident(name) = callee.as_ref() {
                let l   = line.saturating_sub(1) as u32;
                let range = Range {
                    start: Position { line: l, character: 0 },
                    end:   Position { line: l, character: name.len() as u32 },
                };
                out.entry(name.clone()).or_default().push(range);
            }
            collect_calls_in_expr(callee, out);
            for a in args { collect_calls_in_expr(a, out); }
        }
        Expr::BinOp { left, right, .. } | Expr::NullCoalesce { left, right, .. } => {
            collect_calls_in_expr(left, out);
            collect_calls_in_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } => collect_calls_in_expr(operand, out),
        Expr::Index { object, index, .. } => {
            collect_calls_in_expr(object, out);
            collect_calls_in_expr(index,  out);
        }
        Expr::Field   { object, .. } => collect_calls_in_expr(object, out),
        Expr::Array(elems) => {
            for e in elems {
                match e {
                    alcaparra::parser::ast::ArrayElement::Expr(x) |
                    alcaparra::parser::ast::ArrayElement::Spread(x) => collect_calls_in_expr(x, out),
                }
            }
        }
        Expr::Object(fields) => {
            for f in fields {
                match f {
                    alcaparra::parser::ast::ObjectField::Named(_, v) |
                    alcaparra::parser::ast::ObjectField::Spread(v) => collect_calls_in_expr(v, out),
                }
            }
        }
        _ => {}
    }
}

/// Busca la URI y rango de definición de `fn_name` en el archivo actual o en los importados.
fn find_callee_location(
    name:   &str,
    source: &str,
    config: Option<&ProjectConfig>,
) -> Option<(Url, Range)> {
    if let Some(loc) = symbols::find_fn_anywhere(source, name, config) {
        // Encontrada en algún archivo; necesitamos la URI
        // Si está en el fuente actual, buscamos la línea
        if loc.source == source {
            let line = symbols::find_fn_line(source, name)?;
            let l = line.saturating_sub(1) as u32;
            // Necesitamos la URI del archivo actual — la reconstruimos desde config
            let uri = config
                .and_then(|c| Url::from_file_path(c.root.join("main.caper")).ok())
                .unwrap_or_else(|| Url::parse("file:///unknown").unwrap());
            let range = Range {
                start: Position { line: l, character: 0 },
                end:   Position { line: l, character: name.len() as u32 },
            };
            return Some((uri, range));
        }
        // Función en archivo externo — buscar el archivo en los aliases
        if let Some(cfg) = config {
            for (_alias, dir) in &cfg.aliases {
                for file_path in wsymbols::find_caper_files(dir) {
                    if let Ok(ext) = std::fs::read_to_string(&file_path) {
                        if ext == loc.source {
                            if let Ok(uri) = Url::from_file_path(&file_path) {
                                let line = symbols::find_fn_line(&loc.source, name)
                                    .unwrap_or(1);
                                let l = line.saturating_sub(1) as u32;
                                let range = Range {
                                    start: Position { line: l, character: 0 },
                                    end:   Position { line: l, character: name.len() as u32 },
                                };
                                return Some((uri, range));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
