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
    #[error(
        "plugin `{plugin}` was built for `{found}` but this host speaks `manang:plugin@{expected}`; rebuild it against the current WIT"
    )]
    IncompatibleApi {
        plugin: String,
        found: String,
        expected: &'static str,
    },
    #[error("plugin `{plugin}` could not be instantiated: {reason}")]
    Instantiate { plugin: String, reason: String },
    #[error("manifest for `{0}` declares tier `native`; it cannot be loaded as WASM")]
    WrongTier(String),
    #[error(transparent)]
    Manifest(#[from] manang_kernel::ManifestError),
}

/// The precompiled-component cache could not be opened.
///
/// Cache problems while loading are never fatal: the host recompiles instead.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("cache directory `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("cache key `{path}` is not {expected} bytes")]
    BadKey { path: String, expected: usize },
    #[error("no system randomness for the cache key: {0}")]
    Random(String),
}
