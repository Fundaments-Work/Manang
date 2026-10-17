//! Structured error types for the kernel.

use thiserror::Error;

/// A permission entry in a manifest could not be understood.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityError {
    #[error("permission `{0}` must have the form `<read|write>:<scope>`")]
    Malformed(String),
    #[error("permission `{0}` has unknown access `{1}`; expected `read` or `write`")]
    UnknownAccess(String, String),
    #[error("permission `{0}` has an empty or unsafe scope")]
    UnsafeScope(String),
    #[error("network permission `{0}` must be a bare host name")]
    BadHost(String),
}

/// A `manifest.toml` is invalid.
#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("plugin name `{0}` must be lowercase kebab-case starting with a letter")]
    BadName(String),
    #[error("command `{command}` must be namespaced as `{plugin}.<name>`")]
    UnnamespacedCommand { plugin: String, command: String },
    #[error("command `{0}` is declared more than once")]
    DuplicateCommand(String),
    #[error(transparent)]
    Capability(#[from] CapabilityError),
}

/// A plugin refused or failed to handle a request.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin `{plugin}` lacks the capability {capability}")]
    CapabilityDenied { plugin: String, capability: String },
    #[error("plugin does not handle `{0}`")]
    UnknownCommand(String),
    #[error("{0}")]
    Failed(String),
}

/// A kernel operation failed.
#[derive(Debug, Error)]
pub enum KernelError {
    #[error("plugin `{0}` is already registered")]
    DuplicatePlugin(String),
    #[error("command `{command}` is already provided by `{owner}`")]
    CommandTaken { command: String, owner: String },
    #[error("no plugin provides command `{0}`")]
    UnknownCommand(String),
    #[error("no plugin named `{0}` is registered")]
    UnknownPlugin(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Plugin(#[from] PluginError),
}
