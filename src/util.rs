/// Utilidades compartidas para manejo de posiciones LSP.

/// Convierte un offset UTF-16 (`position.character` en LSP) al byte offset equivalente
/// dentro de una línea de texto. Necesario para indexar correctamente en strings con
/// caracteres multibyte (¡, –, emojis, etc.).
///
/// Si `utf16_offset` supera el largo de la línea, devuelve `line.len()`.
pub fn utf16_to_byte_offset(line: &str, utf16_offset: usize) -> usize {
    let mut utf16 = 0usize;
    for (byte_idx, ch) in line.char_indices() {
        if utf16 >= utf16_offset { return byte_idx; }
        utf16 += ch.len_utf16();
    }
    line.len()
}
