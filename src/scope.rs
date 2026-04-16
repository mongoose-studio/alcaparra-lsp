/// Análisis de scope: detecta variables/funciones usadas pero no declaradas.
///
/// Estrategia conservadora (sin falsos positivos):
/// - Recolecta TODAS las declaraciones del archivo en un set plano (ignora fronteras de scope).
/// - Reporta como indefinidos los identificadores que no aparecen en ningún scope conocido.
/// - "Conocidos" = declarados en el script + importados via `use` + HOFs + vars de contexto.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf; // usado en extract_fns_from_file

use alcaparra::lexer::{self, Token};
use alcaparra::parser::{
    self,
    ast::{ArrayElement, AssignTarget, ClosureBody, Expr, MatchPattern, ObjectField, Program, Stmt, UseItems},
};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::project_config::ProjectConfig;

/// Pase de análisis de scope sobre el fuente ya parseado.
/// Detecta identificadores no declarados Y reasignaciones de `let`/`const`.
pub fn check_undefined(source: &str, context_keys: &[String], config: Option<&ProjectConfig>) -> Vec<Diagnostic> {
    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };
    let program = match parser::parse(tokens) {
        Ok(p)  => p,
        Err(_) => return vec![],
    };

    let known     = build_known(&program, context_keys, config);
    let immutable = build_immutable(&program);

    let mut undef_errors:  Vec<(String, usize)> = Vec::new();
    let mut imm_errors:    Vec<(String, usize)> = Vec::new();
    let mut redecl_errors: Vec<(String, usize)> = Vec::new();

    check_stmts(&program.body, &known, &immutable, 1, &mut undef_errors, &mut imm_errors);
    check_redeclarations(&program.body, &mut redecl_errors);

    let mut diags: Vec<Diagnostic> = undef_errors
        .into_iter()
        .map(|(name, line)| undefined_diagnostic(&name, line, source))
        .collect();

    diags.extend(imm_errors.into_iter().map(|(n, l)| immutable_diagnostic(&n, l, source)));
    diags.extend(redecl_errors.into_iter().map(|(n, l)| redecl_diagnostic(&n, l, source)));

    diags
}

// ── Conjunto de nombres inmutables ────────────────────────────────────────────

/// Recolecta los nombres que NO pueden ser reasignados: `let` y constantes del header.
fn build_immutable(program: &Program) -> HashSet<String> {
    let mut imm = HashSet::new();

    // Constantes del header (`const TASA = ...`)
    if let Some(header) = &program.header {
        for c in &header.constants {
            imm.insert(c.name.clone());
        }
    }

    // Variables `let` del cuerpo (flat scan recursivo)
    collect_immutables(&program.body, &mut imm);
    imm
}

fn collect_immutables(stmts: &[Stmt], imm: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, .. } => { imm.insert(name.clone()); }
            Stmt::FnDecl { body, .. } => collect_immutables(body, imm),
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                collect_immutables(then_block, imm);
                for (_, b) in else_if_clauses { collect_immutables(b, imm); }
                if let Some(b) = else_block { collect_immutables(b, imm); }
            }
            Stmt::While   { body, .. } => collect_immutables(body, imm),
            Stmt::Foreach { body, .. } => collect_immutables(body, imm),
            Stmt::TryCatch { try_block, catch_block, .. } => {
                collect_immutables(try_block, imm);
                collect_immutables(catch_block, imm);
            }
            _ => {}
        }
    }
}

// ── Construcción del set de nombres conocidos ─────────────────────────────────

fn build_known(program: &Program, context_keys: &[String], config: Option<&ProjectConfig>) -> HashSet<String> {
    let mut known = HashSet::new();

    // HOFs siempre disponibles (la VM los intercepta antes del dispatch)
    for name in alcaparra::stdlib::arrays::HOF_NAMES {
        known.insert(name.to_string());
    }

    // Variables de contexto inyectadas por el host / .capercfg
    for key in context_keys {
        known.insert(key.clone());
    }

    // Constantes del header
    if let Some(header) = &program.header {
        for c in &header.constants {
            known.insert(c.name.clone());
        }
    }

    // Todo lo declarado en el cuerpo principal (flat, conservador)
    collect_decls(&program.body, &mut known, config);

    known
}

/// Recorre sentencias y agrega al set todos los nombres que declaran.
fn collect_decls(stmts: &[Stmt], known: &mut HashSet<String>, config: Option<&ProjectConfig>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, .. } | Stmt::Var { name, .. } => {
                known.insert(name.clone());
            }
            Stmt::FnDecl { name, params, body, .. } => {
                known.insert(name.clone());
                for p in params { known.insert(p.clone()); }
                collect_decls(body, known, config);
            }
            Stmt::Use { path, items, .. } => {
                match items {
                    UseItems::Named(fns) => {
                        // Funciona tanto para stdlib como para archivos externos:
                        // `use @scripts.calculos.{ calcular, otro }` → agrega "calcular", "otro"
                        for f in fns { known.insert(f.clone()); }
                    }
                    UseItems::Single => {
                        // `use std.math.round` o `use @scripts.calculos.calcular`
                        // → importa el último segmento del path
                        if let Some(last) = path.last() {
                            known.insert(last.clone());
                        }
                    }
                    UseItems::All => {
                        // `use std.math` → todas las funciones del módulo stdlib
                        let module_path = path.join(".");
                        for m in alcaparra::stdlib::MODULES {
                            if m.path == module_path {
                                for f in m.functions {
                                    known.insert(f.to_string());
                                }
                            }
                        }
                        // `use @alias.archivo` → todas las funciones del .caper externo
                        if let Some(cfg) = config {
                            if let Some(file_path) = cfg.resolve_use_path(path) {
                                for name in extract_fns_from_file(&file_path) {
                                    known.insert(name);
                                }
                            }
                        }
                    }
                }
            }
            Stmt::Foreach { key, value, body, .. } => {
                if let Some(k) = key { known.insert(k.clone()); }
                known.insert(value.clone());
                collect_decls(body, known, config);
            }
            Stmt::TryCatch { try_block, error_var, catch_block, .. } => {
                known.insert(error_var.clone());
                collect_decls(try_block, known, config);
                collect_decls(catch_block, known, config);
            }
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                collect_decls(then_block, known, config);
                for (_, b) in else_if_clauses { collect_decls(b, known, config); }
                if let Some(b) = else_block { collect_decls(b, known, config); }
            }
            Stmt::While { body, .. } => collect_decls(body, known, config),
            _ => {}
        }
    }
    // Recoger también params de closures en expresiones
    collect_decls_exprs(stmts, known);
}

fn collect_decls_exprs(stmts: &[Stmt], known: &mut HashSet<String>) {
    for stmt in stmts {
        let exprs = stmt_exprs(stmt);
        for expr in exprs {
            collect_closure_params(expr, known);
        }
    }
}

fn collect_closure_params(expr: &Expr, known: &mut HashSet<String>) {
    match expr {
        Expr::Closure { params, body, .. } => {
            for p in params { known.insert(p.clone()); }
            match body {
                ClosureBody::Expr(e)    => collect_closure_params(e, known),
                ClosureBody::Block(stmts) => collect_decls(stmts, known, None),
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_closure_params(callee, known);
            for a in args { collect_closure_params(a, known); }
        }
        Expr::BinOp { left, right, .. } | Expr::NullCoalesce { left, right, .. } => {
            collect_closure_params(left, known);
            collect_closure_params(right, known);
        }
        Expr::UnaryOp { operand, .. } => collect_closure_params(operand, known),
        Expr::Array(elems) => {
            for e in elems {
                match e {
                    ArrayElement::Expr(x) | ArrayElement::Spread(x) => collect_closure_params(x, known),
                }
            }
        }
        Expr::Object(fields) => {
            for f in fields {
                match f {
                    ObjectField::Named(_, v) | ObjectField::Spread(v) => collect_closure_params(v, known),
                }
            }
        }
        Expr::Index { object, index, .. } => {
            collect_closure_params(object, known);
            collect_closure_params(index, known);
        }
        Expr::Field { object, .. } => collect_closure_params(object, known),
        _ => {}
    }
}

// ── Comprobación de identificadores indefinidos y reasignaciones inmutables ───

fn check_stmts(
    stmts:      &[Stmt],
    known:      &HashSet<String>,
    immutable:  &HashSet<String>,
    line:       usize,
    undef:      &mut Vec<(String, usize)>,
    imm_assign: &mut Vec<(String, usize)>,
) {
    for stmt in stmts {
        check_stmt(stmt, known, immutable, line, undef, imm_assign);
    }
}

fn check_stmt(
    stmt:       &Stmt,
    known:      &HashSet<String>,
    immutable:  &HashSet<String>,
    _line:      usize,
    undef:      &mut Vec<(String, usize)>,
    imm_assign: &mut Vec<(String, usize)>,
) {
    match stmt {
        Stmt::Let  { value, line, .. } |
        Stmt::Var  { value, line, .. } |
        Stmt::Emit { value, line, .. } |
        Stmt::Throw { value, line, .. } => {
            check_expr(value, known, immutable, *line, undef, imm_assign);
        }
        Stmt::Assign { value, target, line, .. } => {
            check_expr(value, known, immutable, *line, undef, imm_assign);
            if let AssignTarget::Ident(name) = target {
                if !known.contains(name.as_str()) {
                    undef.push((name.clone(), *line));
                } else if immutable.contains(name.as_str()) {
                    imm_assign.push((name.clone(), *line));
                }
            }
        }
        Stmt::If { condition, then_block, else_if_clauses, else_block, line } => {
            check_expr(condition, known, immutable, *line, undef, imm_assign);
            check_stmts(then_block, known, immutable, *line, undef, imm_assign);
            for (cond, block) in else_if_clauses {
                check_expr(cond, known, immutable, *line, undef, imm_assign);
                check_stmts(block, known, immutable, *line, undef, imm_assign);
            }
            if let Some(block) = else_block {
                check_stmts(block, known, immutable, *line, undef, imm_assign);
            }
        }
        Stmt::While { condition, body, line } => {
            check_expr(condition, known, immutable, *line, undef, imm_assign);
            check_stmts(body, known, immutable, *line, undef, imm_assign);
        }
        Stmt::Foreach { iter, body, line, .. } => {
            check_expr(iter, known, immutable, *line, undef, imm_assign);
            check_stmts(body, known, immutable, *line, undef, imm_assign);
        }
        Stmt::FnDecl { body, line, .. } => {
            check_stmts(body, known, immutable, *line, undef, imm_assign);
        }
        Stmt::TryCatch { try_block, catch_block, line, .. } => {
            check_stmts(try_block,  known, immutable, *line, undef, imm_assign);
            check_stmts(catch_block, known, immutable, *line, undef, imm_assign);
        }
        Stmt::ExprStmt(expr) => check_expr(expr, known, immutable, 0, undef, imm_assign),
        Stmt::Use { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn check_expr(
    expr:       &Expr,
    known:      &HashSet<String>,
    immutable:  &HashSet<String>,
    line:       usize,
    undef:      &mut Vec<(String, usize)>,
    imm_assign: &mut Vec<(String, usize)>,
) {
    match expr {
        Expr::Ident(name) => {
            if !known.contains(name.as_str()) {
                undef.push((name.clone(), line));
            }
        }
        Expr::BinOp { left, right, line: l, .. } => {
            check_expr(left,  known, immutable, *l, undef, imm_assign);
            check_expr(right, known, immutable, *l, undef, imm_assign);
        }
        Expr::UnaryOp { operand, line: l, .. } =>
            check_expr(operand, known, immutable, *l, undef, imm_assign),
        Expr::NullCoalesce { left, right, line: l } => {
            check_expr(left,  known, immutable, *l, undef, imm_assign);
            check_expr(right, known, immutable, *l, undef, imm_assign);
        }
        Expr::Call { callee, args, line: l } => {
            check_expr(callee, known, immutable, *l, undef, imm_assign);
            for arg in args { check_expr(arg, known, immutable, *l, undef, imm_assign); }
        }
        Expr::Index { object, index, line: l } => {
            check_expr(object, known, immutable, *l, undef, imm_assign);
            check_expr(index,  known, immutable, *l, undef, imm_assign);
        }
        Expr::Field { object, line: l, .. } =>
            check_expr(object, known, immutable, *l, undef, imm_assign),
        Expr::Range { start, end, line: l } => {
            check_expr(start, known, immutable, *l, undef, imm_assign);
            check_expr(end,   known, immutable, *l, undef, imm_assign);
        }
        Expr::Array(elems) => {
            for e in elems {
                match e {
                    ArrayElement::Expr(x) | ArrayElement::Spread(x) =>
                        check_expr(x, known, immutable, line, undef, imm_assign),
                }
            }
        }
        Expr::Object(fields) => {
            for f in fields {
                match f {
                    ObjectField::Named(_, v) | ObjectField::Spread(v) =>
                        check_expr(v, known, immutable, line, undef, imm_assign),
                }
            }
        }
        Expr::Closure { params, body, line: l } => {
            let _ = params;
            match body {
                ClosureBody::Expr(e)      => check_expr(e, known, immutable, *l, undef, imm_assign),
                ClosureBody::Block(stmts) => check_stmts(stmts, known, immutable, *l, undef, imm_assign),
            }
        }
        Expr::IfExpr { condition, then_block, else_block, line: l } => {
            check_expr(condition, known, immutable, *l, undef, imm_assign);
            check_stmts(then_block, known, immutable, *l, undef, imm_assign);
            check_stmts(else_block, known, immutable, *l, undef, imm_assign);
        }
        Expr::Match { subject, arms, line: l } => {
            check_expr(subject, known, immutable, *l, undef, imm_assign);
            for arm in arms {
                // El binding `n if n <= ...` introduce `n` como variable local del arm.
                // Crear un known extendido solo para este arm.
                if let MatchPattern::Binding(binding) = &arm.pattern {
                    let mut arm_known = known.clone();
                    arm_known.insert(binding.clone());
                    if let Some(guard) = &arm.guard {
                        check_expr(guard, &arm_known, immutable, *l, undef, imm_assign);
                    }
                    check_expr(&arm.body, &arm_known, immutable, *l, undef, imm_assign);
                } else {
                    if let Some(guard) = &arm.guard {
                        check_expr(guard, known, immutable, *l, undef, imm_assign);
                    }
                    check_expr(&arm.body, known, immutable, *l, undef, imm_assign);
                }
            }
        }
        Expr::TryCatchExpr { try_block, catch_block, line: l, .. } => {
            check_stmts(try_block,   known, immutable, *l, undef, imm_assign);
            check_stmts(catch_block, known, immutable, *l, undef, imm_assign);
        }
        Expr::Number(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Null => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extrae las expresiones directas de una sentencia (sin entrar en sub-bloques).
fn stmt_exprs(stmt: &Stmt) -> Vec<&Expr> {
    match stmt {
        Stmt::Let  { value, .. } | Stmt::Var { value, .. } |
        Stmt::Emit { value, .. } | Stmt::Throw { value, .. } => vec![value],
        Stmt::Assign { value, .. } => vec![value],
        Stmt::ExprStmt(e) => vec![e],
        _ => vec![],
    }
}

/// Lee un archivo `.caper` externo y extrae los nombres de todas sus funciones declaradas.
/// Falla silenciosamente (devuelve vacío) si el archivo no existe o tiene errores de parse.
fn extract_fns_from_file(path: &PathBuf) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c)  => c,
        Err(_) => return vec![],
    };
    let tokens = match lexer::tokenize(&content) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };
    let program = match parser::parse(tokens) {
        Ok(p)  => p,
        Err(_) => return vec![],
    };

    let mut names = Vec::new();
    collect_fn_names_from_stmts(&program.body, &mut names);
    names
}

fn collect_fn_names_from_stmts(stmts: &[Stmt], out: &mut Vec<String>) {
    for stmt in stmts {
        if let Stmt::FnDecl { name, body, .. } = stmt {
            out.push(name.clone());
            collect_fn_names_from_stmts(body, out);
        }
    }
}

/// Detecta redeclaraciones de `let`/`var`/`fn` en el mismo bloque.
/// El shadowing entre bloques anidados (frames distintos) sí está permitido — igual que el runtime.
fn check_redeclarations(stmts: &[Stmt], out: &mut Vec<(String, usize)>) {
    let mut seen: HashMap<String, usize> = HashMap::new(); // nombre → línea primera decl

    for stmt in stmts {
        match stmt {
            Stmt::Let { name, line, .. } | Stmt::Var { name, line, .. } => {
                if seen.contains_key(name) {
                    out.push((name.clone(), *line));
                } else {
                    seen.insert(name.clone(), *line);
                }
            }
            Stmt::FnDecl { name, body, line, .. } => {
                if seen.contains_key(name) {
                    out.push((name.clone(), *line));
                } else {
                    seen.insert(name.clone(), *line);
                }
                // El cuerpo de la función es un frame nuevo → revisar por separado
                check_redeclarations(body, out);
            }
            // Cada bloque anidado es un frame nuevo (el runtime hace push/pop)
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                check_redeclarations(then_block, out);
                for (_, b) in else_if_clauses { check_redeclarations(b, out); }
                if let Some(b) = else_block { check_redeclarations(b, out); }
            }
            Stmt::While   { body, .. } => check_redeclarations(body, out),
            Stmt::Foreach { body, .. } => check_redeclarations(body, out),
            Stmt::TryCatch { try_block, catch_block, .. } => {
                check_redeclarations(try_block, out);
                check_redeclarations(catch_block, out);
            }
            _ => {}
        }
    }
}

/// Encuentra la columna (en UTF-16) donde aparece `name` en la línea `line` (1-based) del source.
/// Si no se encuentra, devuelve 0.
fn col_of(source: &str, line: usize, name: &str) -> u32 {
    let line_text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let byte_col  = line_text.find(name).unwrap_or(0);
    // Convertir offset de bytes a unidades UTF-16
    line_text[..byte_col].chars().map(|c| c.len_utf16() as u32).sum()
}

fn undefined_diagnostic(name: &str, line: usize, source: &str) -> Diagnostic {
    let l   = line.saturating_sub(1) as u32;
    let col = col_of(source, line, name);
    Diagnostic {
        range: Range {
            start: Position { line: l, character: col },
            end:   Position { line: l, character: col + name.len() as u32 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code:     Some(NumberOrString::String("UNDEFINED_VARIABLE".to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message:  format!("variable o función no definida: `{name}`"),
        ..Default::default()
    }
}

fn immutable_diagnostic(name: &str, line: usize, source: &str) -> Diagnostic {
    let l   = line.saturating_sub(1) as u32;
    let col = col_of(source, line, name);
    Diagnostic {
        range: Range {
            start: Position { line: l, character: col },
            end:   Position { line: l, character: col + name.len() as u32 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code:     Some(NumberOrString::String("IMMUTABLE_ASSIGN".to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message:  format!("`{name}` es inmutable y no puede ser reasignado (declarado con `let` o `const`)"),
        ..Default::default()
    }
}

fn redecl_diagnostic(name: &str, line: usize, source: &str) -> Diagnostic {
    let l   = line.saturating_sub(1) as u32;
    let col = col_of(source, line, name);
    Diagnostic {
        range: Range {
            start: Position { line: l, character: col },
            end:   Position { line: l, character: col + name.len() as u32 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code:     Some(NumberOrString::String("VARIABLE_REDECLARATION".to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message:  format!("`{name}` ya fue declarada en este scope — usa `var` si necesitas reasignar"),
        ..Default::default()
    }
}

// ── Imports no usados ─────────────────────────────────────────────────────────

/// Detecta sentencias `use` cuyos nombres importados no se usan en ningún lugar del archivo.
/// Emite `UNUSED_IMPORT` como warning por cada nombre importado que no aparece.
pub fn check_unused_imports(source: &str) -> Vec<Diagnostic> {
    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };
    let program = match parser::parse(tokens.clone()) {
        Ok(p)  => p,
        Err(_) => return vec![],
    };

    // Líneas donde hay sentencias `use` (1-based) — excluidas del conteo de usos
    let use_lines: HashSet<usize> = program.body.iter()
        .filter_map(|s| if let Stmt::Use { line, .. } = s { Some(*line) } else { None })
        .collect();

    // Todos los identificadores usados fuera de las líneas `use`
    let used: HashSet<String> = tokens.iter()
        .filter(|t| !use_lines.contains(&t.line))
        .filter_map(|t| if let Token::Ident(n) = &t.token { Some(n.clone()) } else { None })
        .collect();

    let mut diags = Vec::new();

    for stmt in &program.body {
        let Stmt::Use { path, items, line } = stmt else { continue };

        let first = path.first().map(|s| s.as_str()).unwrap_or("");

        // Para UseItems::All no reportamos (no sabemos qué funciones se usan del módulo)
        let imported: Vec<&String> = match items {
            UseItems::Named(fns) => fns.iter().collect(),
            UseItems::Single     => path.last().into_iter().collect(),
            UseItems::All        => continue,
        };

        for name in imported {
            if !used.contains(name.as_str()) {
                // Calcular la columna donde aparece el nombre en la línea `use`
                let col = source
                    .lines()
                    .nth(line.saturating_sub(1))
                    .and_then(|l| l.find(name.as_str()))
                    .unwrap_or(0) as u32;
                let l = line.saturating_sub(1) as u32;
                diags.push(Diagnostic {
                    range: Range {
                        start: Position { line: l, character: col },
                        end:   Position { line: l, character: col + name.len() as u32 },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code:     Some(NumberOrString::String("UNUSED_IMPORT".to_string())),
                    source:   Some("alcaparra-lsp".to_string()),
                    message:  format!("`{name}` está importada pero no se usa"),
                    tags:     Some(vec![tower_lsp::lsp_types::DiagnosticTag::UNNECESSARY]),
                    ..Default::default()
                });
            }
        }

        // Silenciar el warning de "first" variable no usada
        let _ = first;
    }

    diags
}
