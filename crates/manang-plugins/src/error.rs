use std::path::PathBuf;

use manang_kernel::{KernelError, ManifestError};
use manang_wasm_host::WasmError;
use thiserror::Error;

/// One plugin directory could not be loaded. Never fatal to other plugins.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("cannot read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{0}` is not a plugin directory (no manifest.toml)")]
    NoManifest(PathBuf),
    #[error("`{path}` is {size} bytes; the limit is {limit}")]
    TooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },
    #[error("directory `{dir}` must be named after the plugin, `{name}`")]
    NameMismatch { dir: String, name: String },
    #[error("plugin entry `{0}` resolves outside the plugin directory")]
    EntryEscapes(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Wasm(#[from] WasmError),
    #[error(transparent)]
    Kernel(#[from] KernelError),
}

/// A failed load, with enough context to show in the UI.
#[derive(Debug)]
pub struct LoadFailure {
    /// The plugin directory.
    pub dir: PathBuf,
    /// The plugin name, if the manifest got far enough to say.
    pub plugin: Option<String>,
    pub error: LoadError,
}

/// The manager itself could not start or complete an operation.
#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("plugins directory `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Runtime(#[from] WasmError),
    #[error("no plugin named `{0}` is loaded")]
    NotLoaded(String),
    #[error(transparent)]
    Kernel(#[from] KernelError),
    #[error("cannot watch `{path}`: {reason}")]
    Watch { path: PathBuf, reason: String },
}
