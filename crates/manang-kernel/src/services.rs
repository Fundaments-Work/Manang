//! Host services that plugins reach through capability-gated interfaces.

/// The active document buffer.
///
/// Implemented by the editor plugin. Hosts for other tiers (such as the WASM
/// host) broker plugin access to it after checking the manifest's grants.
pub trait BufferService: Send + Sync {
    fn read_active(&self) -> Result<String, String>;
    fn write_active(&self, text: &str) -> Result<(), String>;
}
