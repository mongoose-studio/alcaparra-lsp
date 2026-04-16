use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_lsp::lsp_types::Url;

/// Almacena el texto fuente de cada documento abierto por el editor.
#[derive(Clone, Default)]
pub struct Workspace {
    docs: Arc<Mutex<HashMap<Url, String>>>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&self, uri: Url, text: String) {
        self.docs.lock().unwrap().insert(uri, text);
    }

    pub fn update(&self, uri: &Url, text: String) {
        if let Some(entry) = self.docs.lock().unwrap().get_mut(uri) {
            *entry = text;
        }
    }

    pub fn close(&self, uri: &Url) {
        self.docs.lock().unwrap().remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<String> {
        self.docs.lock().unwrap().get(uri).cloned()
    }

    /// Devuelve todos los URIs de documentos actualmente abiertos.
    pub fn all_uris(&self) -> Vec<Url> {
        self.docs.lock().unwrap().keys().cloned().collect()
    }
}
