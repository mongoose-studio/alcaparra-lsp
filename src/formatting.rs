use alcaparra::formatter::{FmtConfig, format};
use tower_lsp::lsp_types::{Position, Range, TextEdit};

use crate::project_config::ProjectConfig;

/// Formatea el documento completo y devuelve un único `TextEdit` que reemplaza
/// todo el contenido. Devuelve `None` si el fuente tiene errores de sintaxis
/// (no podemos formatear código roto), o `Some([])` si ya estaba formateado.
pub fn format_document(source: &str, config: Option<&ProjectConfig>) -> Option<Vec<TextEdit>> {
    let cfg = build_fmt_config(config);

    let formatted = match format(source, &cfg) {
        Ok(f)  => f,
        Err(_) => return None, // Errores de parse → no formatear
    };

    if formatted == source {
        return Some(vec![]); // Sin cambios
    }

    // Rango que cubre todo el documento
    let lines      = source.lines().count() as u32;
    let last_col   = source.lines().last().unwrap_or("").len() as u32;
    let full_range = Range {
        start: Position { line: 0, character: 0 },
        end:   Position { line: lines, character: last_col },
    };

    Some(vec![TextEdit { range: full_range, new_text: formatted }])
}

/// Construye `FmtConfig` desde el `.capercfg` del proyecto si existe,
/// o usa los valores por defecto.
fn build_fmt_config(_config: Option<&ProjectConfig>) -> FmtConfig {
    // TODO Fase 4+: leer la sección `fmt` del `.capercfg` con FmtConfig::from_raw
    // cuando ProjectConfig exponga FmtConfigRaw. Por ahora, defaults.
    FmtConfig::default()
}
