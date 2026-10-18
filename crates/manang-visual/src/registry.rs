//! Which adapters exist, found by id or by file extension.

use std::sync::Arc;

use crate::adapter::{AdapterInfo, FormatAdapter};
use crate::error::VisualError;

/// The registered format adapters, in registration order.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn FormatAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an adapter. Ids are unique.
    pub fn register(&mut self, adapter: Arc<dyn FormatAdapter>) -> Result<(), VisualError> {
        let id = &adapter.info().id;
        if self.get(id).is_some() {
            return Err(VisualError::BadArgs(format!(
                "a format adapter `{id}` is already registered"
            )));
        }
        self.adapters.push(adapter);
        Ok(())
    }

    /// Remove an adapter (a plugin unloaded). Returns whether it existed.
    pub fn unregister(&mut self, id: &str) -> bool {
        let before = self.adapters.len();
        self.adapters.retain(|a| a.info().id != id);
        self.adapters.len() != before
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn FormatAdapter>> {
        self.adapters.iter().find(|a| a.info().id == id).cloned()
    }

    /// The first adapter claiming the extension of `path` (case-insensitive).
    pub fn for_path(&self, path: &str) -> Option<Arc<dyn FormatAdapter>> {
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        self.adapters
            .iter()
            .find(|a| a.info().extensions.iter().any(|e| *e == ext))
            .cloned()
    }

    pub fn infos(&self) -> Vec<AdapterInfo> {
        self.adapters.iter().map(|a| a.info().clone()).collect()
    }
}
