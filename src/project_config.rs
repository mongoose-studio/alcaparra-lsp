use std::collections::HashMap;
use std::path::PathBuf;

use tower_lsp::lsp_types::Url;

/// Información extraída del `.capercfg` más cercano a un documento.
/// Es Clone + Send, a diferencia de `alcaparra::config::CaperConfig`.
#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    /// Directorio raíz del proyecto (donde está el .capercfg).
    pub root: PathBuf,
    /// Nombres de las variables de contexto del proyecto: las de `context: { ... }`
    /// mas las que aporten los `context_file`.
    /// Usadas en completions y goto-definition hacia el .capercfg.
    pub context_keys: Vec<String>,
    /// El contexto del proyecto ya resuelto, con los valores de muestra.
    ///
    /// Los valores importan, no solo las claves: desde v0.2.0 la aritmetica no
    /// convierte texto, asi que un `"sueldo_base": "1850000"` en el .capercfg
    /// —tal como lo serializa PostgreSQL un `numeric`— convierte en error de
    /// ejecucion cualquier `sueldo_base * dias` del script. Con la muestra a la
    /// vista el diagnostico se puede dar en el editor. Ver `crate::numeric`.
    pub context_sample: serde_json::Value,
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

        // El contexto del proyecto puede venir de archivos y no del manifiesto, y el
        // LSP **tiene** que seguirlos: si no, un proyecto que mueve su contexto a un
        // `context_file` pierde el autocompletado y el aviso de aritmetica sobre
        // texto sin que nadie se entere, que es peor que no tener la funcion.
        //
        // Un archivo roto o ausente **no** rompe el editor, al reves que en el CLI:
        // el CLI esta por correr y correr con otro contexto es correr otra cosa, pero
        // el editor solo esta describiendo. Se cae de vuelta al manifiesto, que es
        // peor informacion pero sigue siendo informacion.
        let context = cfg
            .resolved_context(&root)
            .unwrap_or_else(|_| cfg.context.clone());

        let context_keys = match &context {
            serde_json::Value::Object(map) => map.keys().cloned().collect(),
            _ => vec![],
        };

        let aliases = cfg.resolved_aliases(&root);

        Some(Self { root, context_keys, context_sample: context, aliases })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lo que no es negociable: un proyecto que mueve su contexto a un
    /// `context_file` tiene que seguir teniendo autocompletado y valores de
    /// muestra. Si esta prueba falla, la funcion se volvio una trampa.
    #[test]
    fn context_keys_and_sample_come_from_the_context_file_too() {
        let dir = std::env::temp_dir().join("alcaparra-lsp-context-file");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        std::fs::write(
            dir.join(".capercfg"),
            r#"{ "context": { "UF": "36000" }, "context_file": "ctx.json" }"#,
        ).expect("write .capercfg");
        std::fs::write(
            dir.join("ctx.json"),
            r#"{ "SUELDO_BASE": "1850000" }"#,
        ).expect("write ctx.json");
        std::fs::write(dir.join("main.caper"), "main { emit 1; }").expect("write script");

        let uri = Url::from_file_path(dir.join("main.caper")).expect("uri");
        let cfg = ProjectConfig::find_for_uri(&uri).expect("project config");

        assert!(cfg.context_keys.contains(&"UF".to_string()),
            "the manifest key must still be there: {:?}", cfg.context_keys);
        assert!(cfg.context_keys.contains(&"SUELDO_BASE".to_string()),
            "the key from the context file must be there: {:?}", cfg.context_keys);

        // Y la muestra tiene que traer el valor, no solo la clave: es lo que
        // alimenta el aviso de aritmetica sobre texto.
        assert_eq!(cfg.context_sample["SUELDO_BASE"], "1850000");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un `context_file` ausente degrada al manifiesto en vez de dejar al editor
    /// sin nada.
    #[test]
    fn a_missing_context_file_falls_back_to_the_manifest() {
        let dir = std::env::temp_dir().join("alcaparra-lsp-missing-context-file");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        std::fs::write(
            dir.join(".capercfg"),
            r#"{ "context": { "UF": "36000" }, "context_file": "nope.json" }"#,
        ).expect("write .capercfg");
        std::fs::write(dir.join("main.caper"), "main { emit 1; }").expect("write script");

        let uri = Url::from_file_path(dir.join("main.caper")).expect("uri");
        let cfg = ProjectConfig::find_for_uri(&uri).expect("project config");

        assert_eq!(cfg.context_keys, vec!["UF".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
