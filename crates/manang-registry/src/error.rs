use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong talking to the plugin registry. The messages
/// are plain sentences for the person using Manang; [`RegistryError::code`] is
/// the stable machine-readable form.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("the plugin registry is not available in this build of Manang yet")]
    NotConfigured,
    #[error("the plugin registry address must start with https://")]
    InsecureUrl,
    #[error("could not reach {host}: {detail}")]
    Network { host: String, detail: String },
    #[error("{host} answered with HTTP status {status}")]
    BadStatus { host: String, status: u16 },
    #[error("the plugin list could not be verified; nothing was installed")]
    Signature,
    #[error("the plugin list is older than the one Manang already trusts; it was ignored")]
    Stale,
    #[error(
        "the plugin list uses format {found}, which this version of Manang does not understand; update Manang to browse plugins"
    )]
    UnsupportedSchema { found: u64 },
    #[error("the plugin list is not valid: {0}")]
    BadIndex(String),
    #[error("{what} is larger than the {limit} bytes allowed")]
    TooLarge { what: &'static str, limit: u64 },
    #[error("no plugin named `{0}` is in the registry")]
    UnknownPlugin(String),
    #[error("`{id}` has no version {version} in the registry")]
    UnknownVersion { id: String, version: String },
    #[error("`{id}` {version} needs Manang {needs} or newer")]
    NeedsNewerApp {
        id: String,
        version: String,
        needs: String,
    },
    #[error("`{id}` {version} was withdrawn by the registry and cannot be installed")]
    Withdrawn { id: String, version: String },
    #[error("the download stopped after {received} of {expected} bytes")]
    Truncated { received: u64, expected: u64 },
    #[error("the download is not the size the registry lists; nothing was installed")]
    SizeMismatch,
    #[error("the download does not match the checksum the registry lists; nothing was installed")]
    HashMismatch,
    #[error("the download was cancelled")]
    Cancelled,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl RegistryError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "registry_not_configured",
            Self::InsecureUrl => "registry_insecure_url",
            Self::Network { .. } | Self::BadStatus { .. } => "registry_unreachable",
            Self::Signature => "registry_signature",
            Self::Stale => "registry_stale",
            Self::UnsupportedSchema { .. } => "registry_schema",
            Self::BadIndex(_) => "registry_bad_index",
            Self::TooLarge { .. } => "registry_too_large",
            Self::UnknownPlugin(_) => "registry_unknown_plugin",
            Self::UnknownVersion { .. } => "registry_unknown_version",
            Self::NeedsNewerApp { .. } => "registry_needs_newer_app",
            Self::Withdrawn { .. } => "registry_withdrawn",
            Self::Truncated { .. } => "registry_truncated",
            Self::SizeMismatch => "registry_size_mismatch",
            Self::HashMismatch => "registry_hash_mismatch",
            Self::Cancelled => "registry_cancelled",
            Self::Io { .. } => "io",
        }
    }

    /// The registry could not be reached (as opposed to answering badly).
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Self::Network { .. } | Self::BadStatus { .. })
    }
}
