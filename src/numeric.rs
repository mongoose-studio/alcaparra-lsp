//! Aritmética estricta — diagnósticos para la regla que entró en el lenguaje v0.2.0.
//!
//! Hasta v0.1.4 la aritmética convertía texto en silencio: `"1850000" * 26 / 30`
//! daba un número y la primera suma lo volvía a concatenar, así que una
//! liquidación salía mal **sin un solo error**. Desde v0.2.0 `-`, `*`, `/`, `%`
//! y `**` exigen números a ambos lados y revientan con `TYPE_ERROR` en la
//! primera operación.
//!
//! Eso convierte un resultado incorrecto en una falla visible, que es lo que se
//! quería, pero la falla aparece recién al ejecutar. Esta pasada la adelanta al
//! editor en los casos en que el servidor **puede** saber que algo es texto:
//!
//! 1. un literal `"..."`;
//! 2. una variable o constante del header ligada a un literal de texto;
//! 3. una clave del `context` del `.capercfg` cuyo valor de muestra es texto
//!    —el caso real: PostgreSQL serializa `numeric` como string y PHP lo
//!    arrastra hasta el JSON—, o un alias de esa clave (`let t = TRABAJADOR;`);
//! 4. una llamada a una función de la stdlib cuya firma retorna `String`.
//!
//! Fuera de esos cuatro casos no hay información de tipos y la pasada calla: es
//! un seguro barato, no un verificador de tipos. Nunca adivina. En particular
//! un parámetro de función es siempre un valor desconocido: no hay inferencia
//! entre funciones.
//!
//! Con la misma evidencia se cubren las comparaciones, que no son nuevas de
//! v0.2.0 pero fallan igual: `<`, `<=`, `>` y `>=` exigen los dos lados del
//! mismo tipo, y `==` entre texto y número da `false` sin avisar.

use std::collections::{HashMap, HashSet};

use alcaparra::lexer;
use alcaparra::parser::{
    self,
    ast::{ArrayElement, AssignTarget, BinOp, ClosureBody, Expr, ObjectField, Stmt, UnaryOp},
};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::catalog;
use crate::project_config::ProjectConfig;

/// Por qué el servidor cree que una expresión es texto.
/// `anchor` es el texto a subrayar en el editor; `sample` el valor, cuando se
/// conoce (solo lo hay para literales y para la muestra del `.capercfg`).
struct Evidence {
    anchor: String,
    reason: String,
    sample: Option<String>,
}

/// Lo mismo, guardado contra un nombre: de dónde sacó su texto.
#[derive(Clone)]
struct Origin {
    reason: String,
    sample: Option<String>,
}

/// Revisa el fuente y devuelve los diagnósticos de aritmética con texto.
pub fn check_strict_arithmetic(source: &str, config: Option<&ProjectConfig>) -> Vec<Diagnostic> {
    let Ok(tokens) = lexer::tokenize(source) else { return vec![] };
    let Ok(program) = parser::parse(tokens) else { return vec![] };

    let sample = config.map(|c| &c.context_sample);

    // Las funciones declaradas se ven desde cualquier marco; sus nombres tapan a
    // los de la stdlib.
    let mut local_fns = HashSet::new();
    collect_fn_names(&program.body, &mut local_fns);

    // Marco base: el contexto inyectado y las constantes del header. Es lo único
    // que una función ve de afuera —la `ScopeStack` del lenguaje le instala ese
    // frame y nada más—, así que es la semilla de todos los marcos.
    let mut semilla = Ctx::new(source, sample, &local_fns);
    if let Some(header) = &program.header {
        for c in &header.constants {
            semilla.bind(&c.name, &c.value);
        }
    }

    // Las constantes del header se revisan con la semilla ya completa, para que
    // una constante pueda apoyarse en otra declarada antes.
    if let Some(header) = &program.header {
        for c in &header.constants {
            check_expr(&c.value, &mut semilla);
        }
    }
    let mut diags = std::mem::take(&mut semilla.diags);

    // Un marco por cuerpo: el del script y el de cada función.
    //
    // Modelar los marcos importa más de lo que parece. Con un solo set plano,
    // un `foreach t in tramos` dentro de una función tapaba el `let t =
    // TRABAJADOR` de otra, y el diagnóstico se apagaba justo en el script real.
    // Y no es una aproximación: en Alcaparra una función **no** ve los locales
    // de quien la llama, solo el contexto y las constantes. Las closures sí
    // capturan, y por eso se revisan dentro del marco que las contiene.
    let mut marcos: Vec<(&[String], &[Stmt])> = vec![(&[], &program.body[..])];
    collect_fn_frames(&program.body, &mut marcos);

    for (params, body) in marcos {
        let mut ctx = semilla.clone_seed();
        for p in params {
            ctx.shadow(p);
        }
        collect_stmts(body, &mut ctx);
        check_stmts(body, &mut ctx);
        diags.extend(std::mem::take(&mut ctx.diags));
    }

    diags
}

/// Nombres de todas las funciones declaradas, a cualquier profundidad.
fn collect_fn_names(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::FnDecl { name, body, .. } => {
                out.insert(name.clone());
                collect_fn_names(body, out);
            }
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                collect_fn_names(then_block, out);
                for (_, b) in else_if_clauses { collect_fn_names(b, out); }
                if let Some(b) = else_block { collect_fn_names(b, out); }
            }
            Stmt::While { body, .. } | Stmt::Foreach { body, .. } => collect_fn_names(body, out),
            Stmt::TryCatch { try_block, catch_block, .. } => {
                collect_fn_names(try_block, out);
                collect_fn_names(catch_block, out);
            }
            _ => {}
        }
    }
}

/// Cada función declarada es un marco aparte: `(parámetros, cuerpo)`.
fn collect_fn_frames<'s>(stmts: &'s [Stmt], out: &mut Vec<(&'s [String], &'s [Stmt])>) {
    for stmt in stmts {
        match stmt {
            Stmt::FnDecl { params, body, .. } => {
                out.push((&params[..], &body[..]));
                collect_fn_frames(body, out);
            }
            Stmt::If { then_block, else_if_clauses, else_block, .. } => {
                collect_fn_frames(then_block, out);
                for (_, b) in else_if_clauses { collect_fn_frames(b, out); }
                if let Some(b) = else_block { collect_fn_frames(b, out); }
            }
            Stmt::While { body, .. } | Stmt::Foreach { body, .. } => collect_fn_frames(body, out),
            Stmt::TryCatch { try_block, catch_block, .. } => {
                collect_fn_frames(try_block, out);
                collect_fn_frames(catch_block, out);
            }
            _ => {}
        }
    }
}

// ── Estado de la pasada ──────────────────────────────────────────────────────

struct Ctx<'a> {
    source: &'a str,
    /// El bloque `context` del `.capercfg`, si el documento pertenece a un proyecto.
    context_sample: Option<&'a serde_json::Value>,
    /// Nombres ligados a un nodo del contexto, con la ruta legible de ese nodo.
    /// `let t = TRABAJADOR;` deja `t → (nodo, "TRABAJADOR")`.
    nodes: HashMap<String, (&'a serde_json::Value, String)>,
    /// Nombres ligados a texto que no viene del contexto: un literal, o una
    /// cadena de `let` que arranca en uno.
    literals: HashMap<String, Origin>,
    /// Nombres que en algún punto se ligan a otra cosa, o que son parámetro de
    /// función, variable de `foreach` o de `catch`. Un nombre acá no se evalúa:
    /// el servidor no hace análisis de flujo y preferimos callar antes que
    /// marcar un error que no existe.
    shadowed: HashSet<String>,
    /// Funciones declaradas en el propio script. Tapan a la stdlib del mismo
    /// nombre, así que su tipo de retorno es desconocido.
    local_fns: &'a HashSet<String>,
    diags: Vec<Diagnostic>,
}

impl<'a> Ctx<'a> {
    fn new(
        source: &'a str,
        context_sample: Option<&'a serde_json::Value>,
        local_fns: &'a HashSet<String>,
    ) -> Self {
        Self {
            source,
            context_sample,
            nodes: HashMap::new(),
            literals: HashMap::new(),
            shadowed: HashSet::new(),
            local_fns,
            diags: Vec::new(),
        }
    }

    /// Un marco nuevo que arranca de lo visible desde afuera: el contexto y las
    /// constantes del header. No arrastra diagnósticos ni nombres tapados.
    fn clone_seed(&self) -> Ctx<'a> {
        Ctx {
            source: self.source,
            context_sample: self.context_sample,
            nodes: self.nodes.clone(),
            literals: self.literals.clone(),
            shadowed: HashSet::new(),
            local_fns: self.local_fns,
            diags: Vec::new(),
        }
    }

    /// Liga `name` a lo que sea que designe `value`.
    ///
    /// Si `value` apunta a un nodo del contexto se recuerda el nodo —así
    /// `let t = TRABAJADOR;` hace resoluble `t.sueldo_base`—. Si es texto por
    /// otra vía se recuerda el origen. Si no es ninguna de las dos, el nombre
    /// queda tapado: el servidor no sabe qué tiene y prefiere callar.
    fn bind(&mut self, name: &str, value: &Expr) {
        if self.shadowed.contains(name) {
            return;
        }
        if let Some((nodo, ruta)) = node_of(value, self) {
            self.literals.remove(name);
            self.nodes.insert(name.to_string(), (nodo, ruta));
            return;
        }
        if let Some(ev) = string_evidence(value, self) {
            self.nodes.remove(name);
            self.literals.insert(
                name.to_string(),
                Origin {
                    reason: match &ev.reason[..] {
                        "es un literal de texto" => format!("`{name}` está ligada a un literal de texto"),
                        otra => format!("`{name}` recibe texto — {otra}"),
                    },
                    sample: ev.sample,
                },
            );
            return;
        }
        self.shadow(name);
    }

    fn shadow(&mut self, name: &str) {
        self.nodes.remove(name);
        self.literals.remove(name);
        self.shadowed.insert(name.to_string());
    }
}

// ── Pasada 1: qué nombres son texto ──────────────────────────────────────────

fn collect_stmts(stmts: &[Stmt], ctx: &mut Ctx) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, value, .. } | Stmt::Var { name, value, .. } => {
                ctx.bind(name, value);
                collect_in_expr(value, ctx);
            }
            Stmt::Assign { target, value, .. } => {
                if let AssignTarget::Ident(name) = target {
                    ctx.bind(name, value);
                }
                collect_in_expr(value, ctx);
            }
            // El cuerpo de una función es otro marco: se revisa aparte, con la
            // semilla limpia. Acá solo se tapa el nombre por si alguien lo
            // reasigna.
            Stmt::FnDecl { .. } => {}
            Stmt::Foreach { key, value, iter, body, .. } => {
                if let Some(k) = key {
                    ctx.shadow(k);
                }
                ctx.shadow(value);
                collect_in_expr(iter, ctx);
                collect_stmts(body, ctx);
            }
            Stmt::TryCatch { try_block, error_var, catch_block, .. } => {
                ctx.shadow(error_var);
                collect_stmts(try_block, ctx);
                collect_stmts(catch_block, ctx);
            }
            Stmt::If { condition, then_block, else_if_clauses, else_block, .. } => {
                collect_in_expr(condition, ctx);
                collect_stmts(then_block, ctx);
                for (cond, block) in else_if_clauses {
                    collect_in_expr(cond, ctx);
                    collect_stmts(block, ctx);
                }
                if let Some(block) = else_block {
                    collect_stmts(block, ctx);
                }
            }
            Stmt::While { condition, body, .. } => {
                collect_in_expr(condition, ctx);
                collect_stmts(body, ctx);
            }
            Stmt::Emit { value, .. } | Stmt::Throw { value, .. } => collect_in_expr(value, ctx),
            Stmt::ExprStmt(e) => collect_in_expr(e, ctx),
            Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Use { .. } => {}
        }
    }
}

/// Solo busca closures: sus parámetros también tapan nombres de afuera.
fn collect_in_expr(expr: &Expr, ctx: &mut Ctx) {
    for_each_subexpr(expr, &mut |e, ctx| {
        match e {
            Expr::Closure { params, body, .. } => {
                for p in params {
                    ctx.shadow(p);
                }
                if let ClosureBody::Block(stmts) = body {
                    collect_stmts(stmts, ctx);
                }
            }
            Expr::IfExpr { then_block, else_block, .. } => {
                collect_stmts(then_block, ctx);
                collect_stmts(else_block, ctx);
            }
            Expr::TryCatchExpr { try_block, error_var, catch_block, .. } => {
                ctx.shadow(error_var);
                collect_stmts(try_block, ctx);
                collect_stmts(catch_block, ctx);
            }
            _ => {}
        }
    }, ctx);
}

// ── Pasada 2: la revisión ────────────────────────────────────────────────────

fn check_stmts(stmts: &[Stmt], ctx: &mut Ctx) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Var { value, .. } | Stmt::Assign { value, .. }
            | Stmt::Emit { value, .. } | Stmt::Throw { value, .. } => check_expr(value, ctx),
            Stmt::ExprStmt(e) => check_expr(e, ctx),
            // Otro marco — ver `check_strict_arithmetic`.
            Stmt::FnDecl { .. } => {}
            Stmt::While { condition, body, .. } => {
                check_expr(condition, ctx);
                check_stmts(body, ctx);
            }
            Stmt::Foreach { iter, body, .. } => {
                check_expr(iter, ctx);
                check_stmts(body, ctx);
            }
            Stmt::TryCatch { try_block, catch_block, .. } => {
                check_stmts(try_block, ctx);
                check_stmts(catch_block, ctx);
            }
            Stmt::If { condition, then_block, else_if_clauses, else_block, .. } => {
                check_expr(condition, ctx);
                check_stmts(then_block, ctx);
                for (cond, block) in else_if_clauses {
                    check_expr(cond, ctx);
                    check_stmts(block, ctx);
                }
                if let Some(block) = else_block {
                    check_stmts(block, ctx);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Use { .. } => {}
        }
    }
}

fn check_expr(expr: &Expr, ctx: &mut Ctx) {
    for_each_subexpr(expr, &mut |e, ctx| {
        match e {
            Expr::BinOp { op, left, right, line } => check_binop(op, left, right, *line, ctx),
            Expr::UnaryOp { op: UnaryOp::Neg, operand, line } => {
                if let Some(ev) = string_evidence(operand, ctx) {
                    // Advertencia y no error: los dos motores del lenguaje no se
                    // ponen de acuerdo en `-"5"` —el intérprete de árbol lo
                    // convierte, la VM de bytecode lo rechaza—, así que afirmar
                    // que falla sería mentir la mitad de las veces. Igual no se
                    // escribe a propósito.
                    ctx.diags.push(diag(
                        ctx.source, *line, &ev.anchor,
                        DiagnosticSeverity::WARNING,
                        "NEGACION_DE_TEXTO",
                        format!(
                            "negar texto no está definido: los dos motores de Alcaparra no coinciden. \
                             {} — envuélvelo con `to_number(...)`.",
                            ev.reason
                        ),
                    ));
                }
            }
            Expr::Closure { body: ClosureBody::Block(stmts), .. } => check_stmts(stmts, ctx),
            Expr::IfExpr { then_block, else_block, .. } => {
                check_stmts(then_block, ctx);
                check_stmts(else_block, ctx);
            }
            Expr::TryCatchExpr { try_block, catch_block, .. } => {
                check_stmts(try_block, ctx);
                check_stmts(catch_block, ctx);
            }
            Expr::Match { arms, .. } => {
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        check_expr(guard, ctx);
                    }
                    check_expr(&arm.body, ctx);
                }
            }
            _ => {}
        }
    }, ctx);
}

fn check_binop(op: &BinOp, left: &Expr, right: &Expr, line: usize, ctx: &mut Ctx) {
    let simbolo = match op {
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Pow => "**",
        BinOp::Add => {
            check_suma(left, right, line, ctx);
            return;
        }
        BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
            check_orden(left, right, line, ctx);
            return;
        }
        BinOp::Eq | BinOp::NotEq => {
            check_igualdad(left, right, line, ctx);
            return;
        }
        // Los lógicos trabajan sobre la veracidad del valor, no sobre su tipo.
        BinOp::And | BinOp::Or => return,
    };

    for lado in [left, right] {
        if let Some(ev) = string_evidence(lado, ctx) {
            ctx.diags.push(diag(
                ctx.source, line, &ev.anchor,
                DiagnosticSeverity::ERROR,
                "ARITMETICA_CON_TEXTO",
                format!(
                    "`{simbolo}` exige números a ambos lados: la aritmética de Alcaparra no convierte \
                     texto. {} — envuélvelo con `to_number(...)`.",
                    ev.reason
                ),
            ));
        }
    }
}

/// `+` es la excepción: concatena cuando un lado es texto, y eso es legítimo.
/// Solo avisa cuando el texto **parece un número**, que es justo el caso en que
/// nadie quiso concatenar: `"1850000" + bono` da `"1850000120000"`.
fn check_suma(left: &Expr, right: &Expr, line: usize, ctx: &mut Ctx) {
    for lado in [left, right] {
        let Some(ev) = string_evidence(lado, ctx) else { continue };
        let Some(muestra) = &ev.sample else { continue };
        if !parece_numero(muestra) {
            continue;
        }
        ctx.diags.push(diag(
            ctx.source, line, &ev.anchor,
            DiagnosticSeverity::WARNING,
            "SUMA_QUE_CONCATENA",
            format!(
                "`+` concatena cuando un lado es texto: esto pega los dígitos en vez de sumar. \
                 {} y su valor (`{muestra}`) parece un número — envuélvelo con `to_number(...)`.",
                ev.reason
            ),
        ));
    }
}

/// `<`, `<=`, `>` y `>=` solo comparan número con número o texto con texto; una
/// mezcla es `TYPE_ERROR` («cannot compare string and number»). No es una regla
/// nueva de v0.2.0, pero es la misma familia y se detecta con lo mismo.
fn check_orden(left: &Expr, right: &Expr, line: usize, ctx: &mut Ctx) {
    let Some((texto, numero)) = lados_mezclados(left, right, ctx) else { return };
    ctx.diags.push(diag(
        ctx.source, line, &texto.anchor,
        DiagnosticSeverity::ERROR,
        "COMPARACION_DE_TIPOS_DISTINTOS",
        format!(
            "no se puede comparar texto con número: {} y el otro lado es numérico ({numero}).              Envuélvelo con `to_number(...)`.",
            texto.reason
        ),
    ));
}

/// `==` y `!=` no fallan: comparan tipo y valor, así que texto contra número da
/// `false` siempre. Es un aviso, no un error, pero nadie escribe eso a propósito.
fn check_igualdad(left: &Expr, right: &Expr, line: usize, ctx: &mut Ctx) {
    let Some((texto, numero)) = lados_mezclados(left, right, ctx) else { return };
    ctx.diags.push(diag(
        ctx.source, line, &texto.anchor,
        DiagnosticSeverity::WARNING,
        "COMPARACION_SIEMPRE_FALSA",
        format!(
            "comparar texto con número da `false` siempre, sin error: {} y el otro lado es              numérico ({numero}). Envuélvelo con `to_number(...)`.",
            texto.reason
        ),
    ));
}

/// Devuelve el lado que es texto y la descripción del lado que es número, si la
/// comparación mezcla los dos. Requiere saber **ambos** lados: si de uno no hay
/// información, la pasada calla.
fn lados_mezclados(left: &Expr, right: &Expr, ctx: &Ctx) -> Option<(Evidence, String)> {
    if let (Some(ev), Some(n)) = (string_evidence(left, ctx), number_evidence(right, ctx)) {
        return Some((ev, n));
    }
    if let (Some(ev), Some(n)) = (string_evidence(right, ctx), number_evidence(left, ctx)) {
        return Some((ev, n));
    }
    None
}

/// Lo mismo que `string_evidence`, pero para lo que se sabe numérico: un literal
/// o una clave del contexto que llega como número.
fn number_evidence(expr: &Expr, ctx: &Ctx) -> Option<String> {
    match expr {
        Expr::Number(d) => Some(format!("`{d}`")),
        Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } => {
            let (nodo, ruta) = node_of(expr, ctx)?;
            nodo.as_f64().map(|_| format!("`{ruta}`"))
        }
        _ => None,
    }
}

// ── Qué se sabe que es texto ─────────────────────────────────────────────────

fn string_evidence(expr: &Expr, ctx: &Ctx) -> Option<Evidence> {
    match expr {
        Expr::StringLit(s) => Some(Evidence {
            anchor: format!("\"{s}\""),
            reason: "es un literal de texto".to_string(),
            sample: Some(s.clone()),
        }),

        Expr::Ident(name) if ctx.literals.contains_key(name) => {
            let origen = ctx.literals.get(name)?;
            Some(Evidence {
                anchor: name.clone(),
                reason: origen.reason.clone(),
                sample: origen.sample.clone(),
            })
        }

        // `sueldo_base`, `t.sueldo_base`, `TRABAJADOR["sueldo_base"]`: todo lo que
        // se pueda resolver contra la muestra del `.capercfg` sin ejecutar nada.
        Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } => {
            let (nodo, ruta) = node_of(expr, ctx)?;
            let valor  = nodo.as_str()?;
            let anchor = spelling(expr).unwrap_or_else(|| ruta.clone());
            Some(Evidence {
                anchor,
                reason: format!("el `.capercfg` trae `{ruta}` como texto en el contexto"),
                sample: Some(valor.to_string()),
            })
        }

        Expr::Call { callee, .. } => {
            let Expr::Ident(nombre) = callee.as_ref() else { return None };
            if ctx.local_fns.contains(nombre) || !catalog::returns_string(nombre) {
                return None;
            }
            Some(Evidence {
                anchor: format!("{nombre}("),
                reason: format!("`{nombre}()` retorna texto"),
                sample: None,
            })
        }

        _ => None,
    }
}

/// Resuelve `expr` al nodo del `context` del `.capercfg` que designa.
///
/// Devuelve el nodo y la **ruta del contexto** —`TRABAJADOR.sueldo_base`—, que no
/// es necesariamente como está escrita en el script: `let t = TRABAJADOR;` hace
/// que `t.sueldo_base` apunte al mismo lugar, y el script real lo escribe así.
/// Sin seguir ese alias el diagnóstico no sirve para nada.
fn node_of<'a>(expr: &Expr, ctx: &Ctx<'a>) -> Option<(&'a serde_json::Value, String)> {
    match expr {
        Expr::Ident(name) => {
            if ctx.shadowed.contains(name) || ctx.literals.contains_key(name) {
                return None;
            }
            if let Some((nodo, ruta)) = ctx.nodes.get(name) {
                return Some((nodo, ruta.clone()));
            }
            let nodo = ctx.context_sample?.get(name)?;
            Some((nodo, name.clone()))
        }
        Expr::Field { object, field, .. } => {
            let (nodo, ruta) = node_of(object, ctx)?;
            Some((nodo.get(field)?, format!("{ruta}.{field}")))
        }
        Expr::Index { object, index, .. } => {
            let (nodo, ruta) = node_of(object, ctx)?;
            match index.as_ref() {
                Expr::StringLit(clave) => Some((nodo.get(clave)?, format!("{ruta}.{clave}"))),
                Expr::Number(d) => {
                    let i: usize = d.to_string().parse().ok()?;
                    Some((nodo.get(i)?, format!("{ruta}[{i}]")))
                }
                // `arr[i]` con índice variable no se puede resolver sin ejecutar.
                _ => None,
            }
        }
        _ => None,
    }
}

/// Cómo está escrita la expresión en el fuente: `t.sueldo_base`. Sirve para
/// subrayar el texto correcto, que puede diferir de la ruta del contexto.
fn spelling(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Field { object, field, .. } => Some(format!("{}.{field}", spelling(object)?)),
        Expr::Index { object, index, .. } => match index.as_ref() {
            Expr::StringLit(clave) => Some(format!("{}[\"{clave}\"]", spelling(object)?)),
            Expr::Number(d)        => Some(format!("{}[{d}]", spelling(object)?)),
            _ => None,
        },
        _ => None,
    }
}

/// `true` si el texto se vería como número en un JSON: `"1850000"`, `"-0.07"`.
/// Deliberadamente no usa `parse::<f64>()`, que acepta `"inf"` y `"NaN"`.
fn parece_numero(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let cuerpo = s.strip_prefix(['+', '-']).unwrap_or(s);
    !cuerpo.is_empty()
        && cuerpo.chars().any(|c| c.is_ascii_digit())
        && cuerpo.chars().all(|c| c.is_ascii_digit() || c == '.')
        && cuerpo.matches('.').count() <= 1
}

// ── Recorrido genérico ───────────────────────────────────────────────────────

/// Aplica `f` a `expr` y a cada subexpresión, de afuera hacia adentro.
/// No entra a los bloques de sentencias: de eso se encarga cada `f`, que sabe si
/// corresponde recorrerlos.
fn for_each_subexpr(expr: &Expr, f: &mut impl FnMut(&Expr, &mut Ctx), ctx: &mut Ctx) {
    f(expr, ctx);
    match expr {
        Expr::BinOp { left, right, .. } | Expr::NullCoalesce { left, right, .. } => {
            for_each_subexpr(left, f, ctx);
            for_each_subexpr(right, f, ctx);
        }
        Expr::Range { start, end, .. } => {
            for_each_subexpr(start, f, ctx);
            for_each_subexpr(end, f, ctx);
        }
        Expr::UnaryOp { operand, .. } => for_each_subexpr(operand, f, ctx),
        Expr::Index { object, index, .. } => {
            for_each_subexpr(object, f, ctx);
            for_each_subexpr(index, f, ctx);
        }
        Expr::Field { object, .. } => for_each_subexpr(object, f, ctx),
        Expr::Call { callee, args, .. } => {
            for_each_subexpr(callee, f, ctx);
            for arg in args {
                for_each_subexpr(arg, f, ctx);
            }
        }
        Expr::Array(elems) => {
            for e in elems {
                match e {
                    ArrayElement::Expr(x) | ArrayElement::Spread(x) => for_each_subexpr(x, f, ctx),
                }
            }
        }
        Expr::Object(fields) => {
            for field in fields {
                match field {
                    ObjectField::Named(_, v) | ObjectField::Spread(v) => for_each_subexpr(v, f, ctx),
                }
            }
        }
        Expr::Closure { body: ClosureBody::Expr(e), .. } => for_each_subexpr(e, f, ctx),
        Expr::IfExpr { condition, .. } => for_each_subexpr(condition, f, ctx),
        Expr::Match { subject, .. } => for_each_subexpr(subject, f, ctx),
        Expr::Number(_) | Expr::StringLit(_) | Expr::Bool(_) | Expr::Null | Expr::Ident(_)
        | Expr::Closure { .. } | Expr::TryCatchExpr { .. } => {}
    }
}

// ── Construcción del diagnóstico ─────────────────────────────────────────────

/// Subraya `anchor` dentro de la línea `line`. Si no aparece —puede pasar: el
/// texto se reconstruye del AST y el fuente pudo escribirlo con espacios— cae a
/// la línea completa sin sangría, que sigue apuntando al lugar correcto.
fn diag(
    source: &str,
    line: usize,
    anchor: &str,
    severity: DiagnosticSeverity,
    code: &str,
    message: String,
) -> Diagnostic {
    let l = line.saturating_sub(1) as u32;
    let texto = source.lines().nth(l as usize).unwrap_or("");

    let (inicio, largo) = match texto.find(anchor) {
        Some(byte) => (utf16_len(&texto[..byte]), utf16_len(anchor)),
        None => {
            let sangria = texto.len() - texto.trim_start().len();
            (utf16_len(&texto[..sangria]), utf16_len(texto.trim()).max(1))
        }
    };

    Diagnostic {
        range: Range {
            start: Position { line: l, character: inicio },
            end:   Position { line: l, character: inicio + largo },
        },
        severity: Some(severity),
        code:     Some(NumberOrString::String(code.to_string())),
        source:   Some("alcaparra-lsp".to_string()),
        message,
        ..Default::default()
    }
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn con_contexto(source: &str, contexto: serde_json::Value) -> Vec<Diagnostic> {
        let config = ProjectConfig { context_sample: contexto, ..Default::default() };
        check_strict_arithmetic(source, Some(&config))
    }

    fn codigos(diags: &[Diagnostic]) -> Vec<String> {
        diags
            .iter()
            .map(|d| match &d.code {
                Some(NumberOrString::String(s)) => s.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn marca_el_literal_de_texto_en_una_multiplicacion() {
        let diags = check_strict_arithmetic("main { emit \"26\" * 30; }", None);
        assert_eq!(codigos(&diags), ["ARITMETICA_CON_TEXTO"]);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn marca_la_clave_de_contexto_que_llega_como_texto() {
        // El caso real: PostgreSQL serializa `numeric` como string.
        let diags = con_contexto(
            "main {\n  let diario = sueldo_base / 30;\n  emit diario;\n}",
            serde_json::json!({ "sueldo_base": "1850000" }),
        );
        assert_eq!(codigos(&diags), ["ARITMETICA_CON_TEXTO"]);
        assert!(diags[0].message.contains("sueldo_base"), "{}", diags[0].message);
    }

    #[test]
    fn resuelve_la_ruta_dentro_del_contexto() {
        let diags = con_contexto(
            "main { emit trabajador.sueldo * 2; }",
            serde_json::json!({ "trabajador": { "sueldo": "1850000", "nombre": "Ana" } }),
        );
        assert_eq!(codigos(&diags), ["ARITMETICA_CON_TEXTO"]);
    }

    #[test]
    fn no_marca_lo_que_el_contexto_trae_como_numero() {
        let diags = con_contexto(
            "main { emit trabajador.sueldo * 2; }",
            serde_json::json!({ "trabajador": { "sueldo": 1850000 } }),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn marca_una_funcion_de_stdlib_que_retorna_texto() {
        let diags = check_strict_arithmetic("main { emit to_string(5) * 2; }", None);
        assert_eq!(codigos(&diags), ["ARITMETICA_CON_TEXTO"]);
    }

    #[test]
    fn una_funcion_local_del_mismo_nombre_tapa_a_la_stdlib() {
        let source = "fn to_string(v) { emit 7; }\nmain { emit to_string(5) * 2; }";
        assert!(check_strict_arithmetic(source, None).is_empty());
    }

    #[test]
    fn marca_la_constante_del_header_ligada_a_texto() {
        let source = "header {\n  const TASA = \"0.07\";\n}\nmain { emit 1000 * TASA; }";
        assert_eq!(codigos(&check_strict_arithmetic(source, None)), ["ARITMETICA_CON_TEXTO"]);
    }

    #[test]
    fn to_number_apaga_el_diagnostico() {
        let diags = con_contexto(
            "main { emit to_number(sueldo_base) / 30; }",
            serde_json::json!({ "sueldo_base": "1850000" }),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn la_suma_con_texto_numerico_avisa_pero_no_es_error() {
        let diags = con_contexto(
            "main { emit sueldo_base + 120000; }",
            serde_json::json!({ "sueldo_base": "1850000" }),
        );
        assert_eq!(codigos(&diags), ["SUMA_QUE_CONCATENA"]);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn la_concatenacion_de_verdad_no_molesta() {
        // `+` con texto que no parece número es exactamente para lo que está.
        let diags = con_contexto(
            "main { emit \"Hola \" + nombre; }",
            serde_json::json!({ "nombre": "Ana" }),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn un_parametro_que_tapa_el_nombre_hace_callar_la_pasada() {
        // `sueldo_base` acá es el parámetro, no la clave del contexto: sin
        // análisis de flujo, callar es lo correcto.
        let diags = con_contexto(
            "fn diario(sueldo_base) { emit sueldo_base / 30; }\nmain { emit diario(1000); }",
            serde_json::json!({ "sueldo_base": "1850000" }),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn negar_texto_es_advertencia_porque_los_motores_no_coinciden() {
        let diags = check_strict_arithmetic("main { emit -\"5\"; }", None);
        assert_eq!(codigos(&diags), ["NEGACION_DE_TEXTO"]);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn un_foreach_de_otra_funcion_no_tapa_el_alias() {
        // Lo que pasaba con un solo set plano de nombres: el `foreach t` de
        // `tramo_de` apagaba el diagnóstico del `let t = TRABAJADOR` de `main`,
        // que es exactamente como está escrito el script real de liquidación.
        let source = "\
fn tramo_de(tramos) {
    foreach t in tramos { emit t; }
}
main {
    let t = TRABAJADOR;
    emit t.dias_mes - 2;
}";
        let diags = con_contexto(source, serde_json::json!({ "TRABAJADOR": { "dias_mes": "30" } }));
        assert_eq!(codigos(&diags), ["ARITMETICA_CON_TEXTO"]);
        assert!(diags[0].message.contains("TRABAJADOR.dias_mes"), "{}", diags[0].message);
    }

    #[test]
    fn una_funcion_no_ve_los_locales_de_quien_la_llama() {
        // En Alcaparra una función solo recibe el contexto y las constantes: el
        // `let tasa` de `main` no existe dentro de `aplica`, así que ese
        // `tasa * 2` es otra cosa y la pasada no debe opinar.
        let source = "\
fn aplica(tasa) { emit tasa * 2; }
main {
    let tasa = \"0.07\";
    emit aplica(5);
}";
        assert!(check_strict_arithmetic(source, None).is_empty());
    }

    #[test]
    fn comparar_texto_con_numero_es_error() {
        // El motor corta con «cannot compare string and number».
        let diags = con_contexto(
            "main { emit if dias > 0 { 1 } else { 0 }; }",
            serde_json::json!({ "dias": "3" }),
        );
        assert_eq!(codigos(&diags), ["COMPARACION_DE_TIPOS_DISTINTOS"]);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn comparar_texto_con_texto_no_molesta() {
        let diags = con_contexto(
            "main { emit if tipo == \"isapre\" { 1 } else { 0 }; }",
            serde_json::json!({ "tipo": "isapre" }),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn la_igualdad_entre_tipos_distintos_solo_avisa() {
        let diags = con_contexto(
            "main { emit dias == 30; }",
            serde_json::json!({ "dias": "30" }),
        );
        assert_eq!(codigos(&diags), ["COMPARACION_SIEMPRE_FALSA"]);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn comparar_contra_null_no_dispara_nada() {
        // El idioma habitual del script real: `get(mes, "x") == null`.
        let diags = con_contexto(
            "main { emit if tipo == null { 1 } else { 0 }; }",
            serde_json::json!({ "tipo": "isapre" }),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn un_fuente_que_no_parsea_no_produce_ruido() {
        assert!(check_strict_arithmetic("main { emit \"26\" * ;", None).is_empty());
    }

    #[test]
    fn parece_numero_rechaza_infinitos_y_vacios() {
        assert!(parece_numero("1850000"));
        assert!(parece_numero("-0.07"));
        assert!(!parece_numero("inf"));
        assert!(!parece_numero("NaN"));
        assert!(!parece_numero(""));
        assert!(!parece_numero("-"));
        assert!(!parece_numero("1.2.3"));
    }
}
