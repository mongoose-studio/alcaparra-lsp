use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::{catalog, docblock, project_config::ProjectConfig, symbols, util};

/// Genera el contenido hover para la palabra bajo el cursor.
/// Busca en: stdlib → función local (con DocBlock) → función importada de archivo externo.
pub fn hover_at(source: &str, position: Position, config: Option<&ProjectConfig>) -> Option<Hover> {
    let word = word_at_position(source, position)?;

    // ── Stdlib ────────────────────────────────────────────────────────────────
    if let Some(entry) = catalog::lookup(word) {
        let markdown = format!(
            "**`{}`** — _{}_\n\n```caper\n{}\n```\n\n{}",
            word, entry.module, entry.signature, entry.doc
        );
        return Some(hover_markdown(markdown));
    }

    // ── Función local o importada ─────────────────────────────────────────────
    if let Some(fn_loc) = symbols::find_fn_anywhere(source, word, config) {
        let doc      = docblock::find(&fn_loc.source, word).unwrap_or_default();
        let markdown = if doc.is_empty() {
            let signature = format!("fn {}({})", word, fn_loc.params.join(", "));
            format!("```caper\n{}\n```\n\n_Función definida en este script._", signature)
        } else {
            doc.to_hover_markdown(word, &fn_loc.params)
        };
        return Some(hover_markdown(markdown));
    }

    None
}

fn hover_markdown(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

/// Extrae la palabra (identificador) que está bajo el cursor en `source`.
/// Exportada para reutilizar en otros módulos (ej. goto_definition).
pub fn word_at_position<'a>(source: &'a str, position: Position) -> Option<&'a str> {
    let line        = source.lines().nth(position.line as usize)?;
    let byte_cursor = util::utf16_to_byte_offset(line, position.character as usize);

    // Límite del segmento antes del cursor (subslice válido hasta char boundary)
    let before = &line[..byte_cursor];

    // Byte start: buscamos hacia atrás el primer char no-identificador
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|byte_i| {
            // byte_i es el byte start del delimitador; saltamos su longitud UTF-8
            let ch = line[byte_i..].chars().next().unwrap_or('\0');
            byte_i + ch.len_utf8()
        })
        .unwrap_or(0);

    // Byte end: avanzamos desde el cursor el primer char no-identificador
    let after = &line[byte_cursor..];
    let end   = byte_cursor
        + after
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after.len());

    let word = &line[start..end];
    if word.is_empty() { None } else { Some(word) }
}

#[allow(dead_code)]
/// Igual que `word_at_position` pero retorna el `Range` LSP del token.
pub fn word_range_at_position(source: &str, position: Position) -> Option<Range> {
    let line_idx    = position.line;
    let line        = source.lines().nth(line_idx as usize)?;
    let byte_cursor = util::utf16_to_byte_offset(line, position.character as usize);

    let before = &line[..byte_cursor];
    let start_byte = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|byte_i| {
            let ch = line[byte_i..].chars().next().unwrap_or('\0');
            byte_i + ch.len_utf8()
        })
        .unwrap_or(0);

    let after = &line[byte_cursor..];
    let end_byte = byte_cursor
        + after
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after.len());

    if start_byte == end_byte { return None; }

    // Convertir byte offsets a char offsets para el Range LSP
    let start_char = line[..start_byte].chars().count() as u32;
    let end_char   = line[..end_byte  ].chars().count() as u32;

    Some(Range {
        start: Position { line: line_idx, character: start_char },
        end:   Position { line: line_idx, character: end_char   },
    })
}
