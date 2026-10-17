use thiserror::Error;

/// Failure to load a WASM plugin.
#[derive(Debug, Error)]
pub enum WasmError {
    #[error("failed to configure the WASM engine: {0}")]
    Engine(String),
    #[error("plugin `{plugin}` is not a valid component: {reason}")]
    InvalidComponent { plugin: String, reason: String },
    #[error("plugin `{plugin}` imports `{interface}` but its manifest does not grant it")]
    UngrantedImport { plugin: String, interface: String },
    #[error("plugin `{plugin}` imports unknown interface `{interface}`")]
    UnknownImport { plugin: String, interface: String },
    #[error("plugin `{plugin}` could not be instantiated: {reason}")]
    Instantiate { plugin: String, reason: String },
    #[error("manifest for `{0}` declares tier `native`; it cannot be loaded as WASM")]
    WrongTier(String),
    #[error(transparent)]
    Manifest(#[from] manang_kernel::ManifestError),
}
