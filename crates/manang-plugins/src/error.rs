use std::path::PathBuf;

use manang_kernel::{KernelError, ManifestError};
use manang_wasm_host::WasmError;
use thiserror::Error;

use crate::archive::ArchiveError;
use crate::registry::RegistryError;

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
    #[error("blocked by policy: {0}")]
    Policy(String),
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

impl LoadFailure {
    /// The name to show: the plugin's, or its directory's.
    pub fn display_name(&self) -> String {
        self.plugin.clone().unwrap_or_else(|| {
            self.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }
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
    #[error("`{0}` is not a valid plugin name (lowercase letters, digits and `-`)")]
    InvalidName(String),
    #[error("`{0}` is not a plugin folder or .manangplugin archive")]
    InvalidPath(String),
    #[error("no plugin named `{0}` is installed")]
    NotInstalled(String),
    #[error(
        "`{name}` {version} is already installed; install a newer version or uninstall it first"
    )]
    AlreadyInstalled { name: String, version: String },
    #[error("`{0}` is part of the core of Manang and cannot be disabled or removed")]
    Core(String),
    #[error("`{0}` is built in and cannot be uninstalled")]
    BuiltIn(String),
    #[error("`{0}` has nothing waiting for approval")]
    NothingToApprove(String),
    #[error("cannot grant {0}: the plugin does not ask for it")]
    PermissionMismatch(String),
    #[error("`{name}` is blocked by policy: {reason}")]
    Denied { name: String, reason: String },
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error("the plugin is not valid: {0}")]
    Invalid(Box<LoadError>),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("cannot move `{from}` to the trash: {source}")]
    Trash {
        from: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl ManagerError {
    /// A stable machine-readable code: the `[code]` prefix of the message
    /// the kernel commands return.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::Runtime(_) => "runtime",
            Self::NotLoaded(_) => "not_loaded",
            Self::Kernel(_) => "kernel",
            Self::Watch { .. } => "watch",
            Self::InvalidName(_) => "invalid_name",
            Self::InvalidPath(_) => "invalid_path",
            Self::NotInstalled(_) => "not_installed",
            Self::AlreadyInstalled { .. } => "already_installed",
            Self::Core(_) => "core_plugin",
            Self::BuiltIn(_) => "built_in",
            Self::NothingToApprove(_) => "nothing_to_approve",
            Self::PermissionMismatch(_) => "permission_mismatch",
            Self::Denied { .. } => "policy_denied",
            Self::Archive(_) => "archive",
            Self::Invalid(_) => "invalid_plugin",
            Self::Registry(_) => "registry",
            Self::Trash { .. } => "trash",
        }
    }
}
