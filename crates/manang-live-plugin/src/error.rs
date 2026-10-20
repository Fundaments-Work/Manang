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
    /// The session is waiting for someone to answer; edits cannot be sent yet.
    #[error("Waiting for someone in the session to answer. Your file has not been changed.")]
    Waiting,
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
    /// A project command on a single-file session.
    #[error("This is not a project session.")]
    NotProject,
    /// A project session needs the folder to share or to join into.
    #[error("Open a project folder to use live project sessions.")]
    NoProject,
    /// The path names no file of the session.
    #[error("{0} is not in the session.")]
    NotInSession(String),
    /// The path is kept out of the session on this side.
    #[error("{0} is kept out of the session.")]
    Excluded(String),
    /// A path the security policy refuses.
    #[error("{0}")]
    BadPath(String),
    /// A file that is not text cannot be edited live.
    #[error("{0} is not a text file.")]
    NotText(String),
    /// Nothing to resolve: no join plan is waiting.
    #[error("There is nothing to resolve.")]
    NoPlan,
    /// Differing files with no answer.
    #[error("{0} file(s) differ and need a choice.")]
    Unresolved(usize),
    /// More than one of the guest's files would be replaced without the
    /// all-files confirmation.
    #[error("Replacing {0} of your files needs the all-files confirmation.")]
    NeedsConfirmation(usize),
    /// The file was not deleted in this session (or its copy is gone).
    #[error("{0} was not deleted in this session.")]
    NotDeleted(String),
    /// The file is still being fetched.
    #[error("{0} is still being fetched. Try again in a moment.")]
    Fetching(String),
    /// A large rewrite on disk is held until it is answered.
    #[error("A large change on disk is waiting for an answer.")]
    Paused,
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
            Self::Waiting => "waiting",
            Self::Invalid(_) => "invalid",
            Self::Editor(_) => "editor",
            Self::Core(_) => "core",
            Self::Io(_) => "io",
            Self::Stopped => "stopped",
            Self::NotProject => "not-project",
            Self::NoProject => "no-project",
            Self::NotInSession(_) => "not-in-session",
            Self::Excluded(_) => "excluded",
            Self::BadPath(_) => "bad-path",
            Self::NotText(_) => "not-text",
            Self::NoPlan => "no-plan",
            Self::Unresolved(_) => "unresolved",
            Self::NeedsConfirmation(_) => "needs-confirmation",
            Self::NotDeleted(_) => "not-deleted",
            Self::Fetching(_) => "waiting",
            Self::Paused => "paused",
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
