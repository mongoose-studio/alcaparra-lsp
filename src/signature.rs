/// Signature help: muestra la firma de una función mientras el usuario escribe sus argumentos.
///
/// Estrategia:
/// - Escanea el prefijo de la línea actual de derecha a izquierda buscando el `(` sin cerrar.
/// - El identificador antes de ese `(` es la función que se está llamando.
/// - Los `,` al mismo nivel de profundidad determinan el parámetro activo.
/// - Primero busca en el catálogo de stdlib; si no, en las funciones declaradas en el script.
use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel,
    Position, SignatureHelp, SignatureInformation,
};

use crate::{catalog, docblock, project_config::ProjectConfig, symbols, util};

pub fn signature_help(source: &str, position: Position, config: Option<&ProjectConfig>) -> Option<SignatureHelp> {
    let line     = source.lines().nth(position.line as usize)?;
    let byte_off = util::utf16_to_byte_offset(line, position.character as usize).min(line.len());
    let prefix   = &line[..byte_off];

    let (fn_name, active_param) = call_context(prefix)?;

    // ── Buscar en stdlib ──────────────────────────────────────────────────────
    if let Some(entry) = catalog::lookup(&fn_name) {
        let params = params_from_signature(entry.signature);
        let sig = SignatureInformation {
            label: entry.signature.to_string(),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: format!("```caper\n{}\n```\n\n{}", entry.signature, entry.doc),
            })),
            parameters: Some(params),
            active_parameter: None,
        };
        return Some(build(sig, active_param));
    }

    // ── Buscar en funciones locales e importadas ─────────────────────────────
    if let Some(fn_loc) = symbols::find_fn_anywhere(source, &fn_name, config) {
        let params = fn_loc.params;
        let doc    = docblock::find(&fn_loc.source, &fn_name).unwrap_or_default();
        let label  = format!("{}({})", fn_name, params.join(", "));

        let param_infos = params
            .iter()
            .map(|p| {
                let doc_text = doc.param_doc(p).map(|d| {
                    Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: d.to_string(),
                    })
                });
                ParameterInformation {
                    label:         ParameterLabel::Simple(p.clone()),
                    documentation: doc_text,
                }
            })
            .collect();

        let fn_doc = if doc.is_empty() {
            None
        } else {
            Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: doc.to_hover_markdown(&fn_name, &params),
            }))
        };

        let sig = SignatureInformation {
            label,
            documentation: fn_doc,
            parameters:    Some(param_infos),
            active_parameter: None,
        };
        return Some(build(sig, active_param));
    }

    None
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Escanea el prefijo de derecha a izquierda para detectar la llamada activa.
/// Devuelve `(nombre_función, índice_parámetro_activo)`.
fn call_context(prefix: &str) -> Option<(String, u32)> {
    let chars: Vec<char> = prefix.chars().collect();
    let mut depth = 0i32;
    let mut active_param = 0u32;
    let mut i = chars.len();

    while i > 0 {
        i -= 1;
        match chars[i] {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    // Encontramos el `(` de apertura de la llamada actual.
                    // El nombre de la función está justo antes.
                    let before: String = chars[..i].iter().collect();
                    let fn_name = before
                        .trim_end()
                        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .filter(|s| !s.is_empty())?
                        .to_string();
                    return Some((fn_name, active_param));
                }
                depth -= 1;
            }
            ',' if depth == 0 => active_param += 1,
            _ => {}
        }
    }
    None
}

/// Extrae `ParameterInformation` desde un string de firma como `round(valor, decimales) → Number`.
fn params_from_signature(signature: &str) -> Vec<ParameterInformation> {
    let inner = signature
        .find('(')
        .and_then(|s| signature.find(')').map(|e| &signature[s + 1..e]))
        .unwrap_or("");

    if inner.trim().is_empty() {
        return vec![];
    }

    inner
        .split(',')
        .map(|p| ParameterInformation {
            label:         ParameterLabel::Simple(p.trim().to_string()),
            documentation: None,
        })
        .collect()
}

fn build(sig: SignatureInformation, active_param: u32) -> SignatureHelp {
    SignatureHelp {
        signatures:       vec![sig],
        active_signature: Some(0),
        active_parameter: Some(active_param),
    }
}
