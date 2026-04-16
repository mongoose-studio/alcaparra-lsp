use std::collections::HashMap;
use std::path::PathBuf;

use tower_lsp::lsp_types::Url;

/// Información extraída del `.capercfg` más cercano a un documento.
/// Es Clone + Send, a diferencia de `alcaparra::config::CaperConfig`.
#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    /// Directorio raíz del proyecto (donde está el .capercfg).
    pub root: PathBuf,
    /// Nombres de las variables de contexto declaradas en `context: { ... }`.
    /// Usadas en completions y goto-definition hacia el .capercfg.
    pub context_keys: Vec<String>,
    /// Aliases de ruta resueltos para `Interpreter::with_loader`.
    /// Ej: `{ "@formulas" → "/Users/mrojas/proyecto/formulas" }`
    pub aliases: HashMap<String, PathBuf>,
}

impl ProjectConfig {
    /// Convierte un path de `use` en la ruta al archivo `.caper` externo, si aplica.
    ///
    /// - `["@scripts", "calculos"]`  → `<alias_root>/calculos.caper`
    /// - `["@scripts", "sub", "mod"]`→ `<alias_root>/sub/mod.caper`
    /// - `["./ruta", "archivo"]`     → `<root>/ruta/archivo.caper`
    ///
    /// Devuelve `None` para paths stdlib (`std.*`) o alias no configurados.
    pub fn resolve_use_path(&self, path: &[String]) -> Option<PathBuf> {
        let first = path.first()?;

        if first.starts_with('@') {
            // Las claves en self.aliases se guardan con el '@' (ej. "@funcs")
            let alias_root = self.aliases.get(first.as_str())?;
            let rest: PathBuf = path[1..].iter().collect();
            let mut file = alias_root.join(rest);
            file.set_extension("caper");
            return Some(file);
        }

        if first.starts_with('.') {
            let rel: PathBuf = path.iter().collect();
            let mut file = self.root.join(rel);
            file.set_extension("caper");
            return Some(file);
        }

        None
    }

    /// Busca el `.capercfg` más cercano al archivo indicado (sube directorios)
    /// y extrae los campos relevantes para el LSP.
    /// Devuelve `None` si no hay `.capercfg` en ningún directorio padre.
    pub fn find_for_uri(uri: &Url) -> Option<Self> {
        let path = uri.to_file_path().ok()?;
        let start_dir = path.parent()?;

        let (cfg, root) = alcaparra::config::find_config(start_dir)?;

        let context_keys = match &cfg.context {
            serde_json::Value::Object(map) => map.keys().cloned().collect(),
            _ => vec![],
        };

        let aliases = cfg.resolved_aliases(&root);

        Some(Self { root, context_keys, aliases })
    }
}
