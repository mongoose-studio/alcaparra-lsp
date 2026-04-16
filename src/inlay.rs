/// Inlay hints: muestra los nombres de parámetros inline al escribir llamadas a funciones.
///
/// Estrategia basada en el stream de tokens:
/// - Detecta patrones `Ident(name) LParen` → llamada a función
/// - Busca los nombres de parámetros en stdlib/local/externo
/// - Para cada argumento emite un hint `param:` justo antes del token inicial del argumento
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};

use alcaparra::lexer::{self, Token};

use crate::{catalog, project_config::ProjectConfig, symbols};

pub fn inlay_hints(
    source: &str,
    range:  Option<Range>,
    config: Option<&ProjectConfig>,
) -> Vec<InlayHint> {
    let tokens = match lexer::tokenize(source) {
        Ok(t)  => t,
        Err(_) => return vec![],
    };

    let mut hints = Vec::new();
    let n = tokens.len();
    let mut i = 0;

    while i + 1 < n {
        // Busca Ident seguido de LParen
        let Token::Ident(ref fn_name) = tokens[i].token else { i += 1; continue };
        if tokens[i + 1].token != Token::LParen { i += 1; continue }

        let fn_name = fn_name.clone();

        // Resuelve los parámetros (stdlib primero, luego local/externo)
        let params: Option<Vec<String>> = if let Some(entry) = catalog::lookup(&fn_name) {
            Some(params_from_signature(entry.signature))
        } else {
            symbols::find_fn_anywhere(source, &fn_name, config)
                .map(|loc| loc.params)
        };

        let Some(params) = params else { i += 1; continue };
        if params.is_empty() { i += 1; continue }

        // Avanza al interior de la llamada
        i += 2; // consumir Ident + LParen
        let mut depth      = 1usize;
        let mut param_idx  = 0usize;
        let mut arg_start  = true; // ¿estamos esperando el primer token del argumento actual?

        while i < n && depth > 0 {
            let tok = &tokens[i];
            match &tok.token {
                Token::LParen | Token::LBrace | Token::LBracket => {
                    if arg_start && depth == 1 {
                        // El argumento empieza con un delimitador (ej: objeto, array, closure)
                        if let Some(pname) = params.get(param_idx) {
                            if should_show_hint(pname) {
                                if in_range(tok.line, range) {
                                    hints.push(make_hint(tok.line, tok.col, pname));
                                }
                            }
                        }
                        arg_start = false;
                    }
                    depth += 1;
                }
                Token::RParen | Token::RBrace | Token::RBracket => {
                    depth -= 1;
                }
                Token::Comma if depth == 1 => {
                    param_idx += 1;
                    arg_start = true;
                }
                _ if arg_start && depth == 1 => {
                    if let Some(pname) = params.get(param_idx) {
                        if should_show_hint(pname) {
                            if in_range(tok.line, range) {
                                hints.push(make_hint(tok.line, tok.col, pname));
                            }
                        }
                    }
                    arg_start = false;
                }
                _ => {}
            }
            i += 1;
        }
        // i ya apunta al token después del RParen de cierre, seguir desde ahí
    }

    hints
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_hint(line_1based: usize, col_1based: usize, param: &str) -> InlayHint {
    InlayHint {
        position:     Position {
            line:      (line_1based.saturating_sub(1)) as u32,
            character: (col_1based.saturating_sub(1)) as u32,
        },
        label:        InlayHintLabel::String(format!("{}:", param)),
        kind:         Some(InlayHintKind::PARAMETER),
        text_edits:   None,
        tooltip:      None,
        padding_left: None,
        padding_right: Some(true),
        data:         None,
    }
}

/// Omite parámetros de un solo carácter o con nombres genéricos poco informativos.
fn should_show_hint(name: &str) -> bool {
    name.len() > 1 && name != "_"
}

/// Extrae los nombres de parámetros desde una firma como `round(valor, decimales) → Number`.
fn params_from_signature(signature: &str) -> Vec<String> {
    let inner = signature
        .find('(')
        .and_then(|s| signature.find(')').map(|e| &signature[s + 1..e]))
        .unwrap_or("");

    if inner.trim().is_empty() {
        return vec![];
    }

    inner
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Devuelve true si la línea (1-based) cae dentro del rango solicitado (si lo hay).
fn in_range(line_1based: usize, range: Option<Range>) -> bool {
    match range {
        None    => true,
        Some(r) => {
            let l = (line_1based.saturating_sub(1)) as u32;
            l >= r.start.line && l <= r.end.line
        }
    }
}
