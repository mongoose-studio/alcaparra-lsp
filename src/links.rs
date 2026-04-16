/// Document links: convierte los paths de `use @alias.file` en hipervínculos clickables.
use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

use alcaparra::lexer;
use alcaparra::parser::{self, ast::{Stmt, UseItems}};

use crate::project_config::ProjectConfig;

pub fn document_links(
    source: &str,
    config: Option<&ProjectConfig>,
) -> Vec<DocumentLink> {
    let config = match config { Some(c) => c, None => return vec![] };

    let tokens  = match lexer::tokenize(source)  { Ok(t) => t, Err(_) => return vec![] };
    let program = match parser::parse(tokens) { Ok(p) => p, Err(_) => return vec![] };

    let mut links = Vec::new();

    for stmt in &program.body {
        let Stmt::Use { path, items, line } = stmt else { continue };

        // Solo imports externos (no stdlib)
        let first = path.first().map(|s| s.as_str()).unwrap_or("");
        if first == "std" { continue; }

        // Segmentos del path que apuntan al archivo (excluye el nombre de fn en Single)
        let file_parts: &[String] = match items {
            UseItems::Single if path.len() > 1 => &path[..path.len() - 1],
            _ => path,
        };

        let file_path = match config.resolve_use_path(file_parts) { Some(p) => p, None => continue };
        let target_uri = match Url::from_file_path(&file_path) { Ok(u) => u, Err(_) => continue };

        // Encontrar el rango del path en la línea fuente
        let line_text = match source.lines().nth(line.saturating_sub(1)) {
            Some(l) => l,
            None    => continue,
        };

        // El path en texto fuente: todo lo que va desde el primer segmento hasta `{`, `;` o el último segmento
        let path_str = file_parts.join(".");
        let Some(col_start) = line_text.find(&path_str) else { continue };
        let col_end = col_start + path_str.len();

        let l = line.saturating_sub(1) as u32;
        links.push(DocumentLink {
            range: Range {
                start: Position { line: l, character: col_start as u32 },
                end:   Position { line: l, character: col_end   as u32 },
            },
            target:  Some(target_uri),
            tooltip: Some(file_path.to_string_lossy().to_string()),
            data:    None,
        });
    }

    links
}
