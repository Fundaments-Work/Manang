//! Errors of the live plugin, with stable codes for the shell.

use manang_kernel::PluginError;
use serde_json::json;
use thiserror::Error;

/// Everything that can go wrong in a `live.*` command.
#[derive(Debug, Error)]
pub enum LiveHostError {
    /// No relay address is set, so nothing can be shared.
    #[error("Set a relay in Settings to share live.")]
    NoRelay,
    /// The relay address is not usable.
    #[error("The relay address is not valid: {0}")]
    BadRelay(String),
    /// The invite could not be read.
    #[error("This invite is not valid: {0}")]
    BadInvite(String),
    /// A session is already running; one live file per session.
    #[error("A live session is already running. Leave it first.")]
    AlreadyLive,
    /// No session is running.
    #[error("There is no live session.")]
    NotLive,
    /// The joiner's copy differs from the host's start text.
    #[error(
        "Your copy of this file differs from the host's. Joining replaces it with the host's version; your copy is saved first."
    )]
    Differs,
    /// A command argument was missing or malformed.
    #[error("{0}")]
    Invalid(String),
    /// The editor refused a read or write.
    #[error("editor: {0}")]
    Editor(String),
    /// The CRDT or the envelope failed.
    #[error("live core: {0}")]
    Core(String),
    /// Disk I/O (saving the replaced copy).
    #[error("{0}")]
    Io(String),
    /// The session thread is gone.
    #[error("The live session stopped unexpectedly.")]
    Stopped,
}

impl LiveHostError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoRelay => "no-relay",
            Self::BadRelay(_) => "bad-relay",
            Self::BadInvite(_) => "bad-invite",
            Self::AlreadyLive => "already-live",
            Self::NotLive => "not-live",
            Self::Differs => "differs",
            Self::Invalid(_) => "invalid",
            Self::Editor(_) => "editor",
            Self::Core(_) => "core",
            Self::Io(_) => "io",
            Self::Stopped => "stopped",
        }
    }
}

impl From<manang_live::LiveError> for LiveHostError {
    fn from(error: manang_live::LiveError) -> Self {
        Self::Core(error.to_string())
    }
}

impl From<LiveHostError> for PluginError {
    fn from(error: LiveHostError) -> Self {
        // Same shape as the git plugin: the message is JSON the shell decodes.
        PluginError::Failed(
            json!({ "code": error.code(), "message": error.to_string() }).to_string(),
        )
    }
}
