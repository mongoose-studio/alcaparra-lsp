/// Workspace symbols: búsqueda de símbolos en todos los archivos .caper del proyecto.
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{SymbolInformation, Url};

use crate::symbols;

/// Devuelve todos los símbolos del proyecto que coincidan con `query` (substring, case-insensitive).
/// Escanea todos los archivos `.caper` bajo `root`.
pub fn workspace_symbols(root: &Path, query: &str) -> Vec<SymbolInformation> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for file_path in find_caper_files(root) {
        let source = match std::fs::read_to_string(&file_path) {
            Ok(s)  => s,
            Err(_) => continue,
        };
        let uri = match Url::from_file_path(&file_path) {
            Ok(u)  => u,
            Err(_) => continue,
        };

        for sym in symbols::extract(&source) {
            if query_lower.is_empty() || sym.name.to_lowercase().contains(&query_lower) {
                let location = sym.to_location(&uri);
                #[allow(deprecated)]
                results.push(SymbolInformation {
                    name:           sym.name,
                    kind:           sym.kind,
                    location,
                    container_name: None,
                    tags:           None,
                    deprecated:     None,
                });
            }
        }
    }

    results
}

/// Encuentra todos los archivos `.caper` recursivamente bajo `dir`.
pub fn find_caper_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_caper_files(dir, &mut files);
    files
}

fn collect_caper_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e)  => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Ignorar directorios ocultos y `vendors`
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "vendors" { continue; }
            collect_caper_files(&path, out);
        } else if path.extension().map(|e| e == "caper").unwrap_or(false) {
            out.push(path);
        }
    }
}
