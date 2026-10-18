//! Structured errors. Every failure carries a stable machine-readable `code`
//! and a human, actionable message; the kernel boundary renders them as a
//! JSON object string so the UI can branch on `code` without parsing prose.

use manang_kernel::PluginError;
use manang_project::ProjectError;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("no project root is set; pass `root` or call git.open first")]
    NoRoot,
    #[error("`{0}` is not inside a git repository; run git.init to create one")]
    NotARepo(String),
    #[error("`{0}` is already a git repository")]
    AlreadyRepo(String),
    #[error(transparent)]
    Path(#[from] ProjectError),
    #[error("bad arguments: {0}")]
    BadArgs(String),
    #[error("discarding changes is destructive; pass `confirm: true` to proceed")]
    ConfirmRequired,
    #[error(
        "git has no author identity; run `git config --global user.name \"Your Name\"` and `git config --global user.email you@example.com`, then commit again"
    )]
    MissingIdentity,
    #[error("there is nothing staged to commit")]
    NothingToCommit,
    #[error("the commit message is empty")]
    EmptyMessage,
    #[error("the last commit is already on a remote; amending would rewrite published history")]
    AmendPublished,
    #[error("cannot amend: the branch has no commits yet")]
    AmendUnborn,
    #[error("uncommitted changes would be overwritten; commit, stage or discard them first:\n{0}")]
    DirtyTree(String),
    #[error(
        "{} file(s) have changes that restoring would overwrite; save a version first (or pass `save_first`): {}",
        .0.len(),
        .0.join(", ")
    )]
    DirtyPaths(Vec<String>),
    #[error("there is nothing to save: no files have changed since the last version")]
    NothingToSave,
    #[error("`{0}` is not a valid branch name")]
    BadBranchName(String),
    #[error("the system `git` executable was not found on PATH; install git to use this command")]
    GitMissing,
    #[error("git did not finish within {secs}s and was stopped")]
    Timeout { secs: u64 },
    #[error("`{what}` is {size} bytes, over the {limit} byte limit")]
    TooLarge { what: String, size: u64, limit: u64 },
    #[error("git {command} failed (exit {code}): {stderr}")]
    CommandFailed {
        command: String,
        code: i32,
        stderr: String,
    },
    #[error("revision `{0}` was not found")]
    RevNotFound(String),
    #[error("`{0}` does not exist at that revision")]
    PathNotInRev(String),
    #[error("this project is not connected to a shared location yet; add its address first")]
    NoRemote,
    #[error("{0}")]
    BadRemoteUrl(String),
    #[error("there is nothing to share yet; save a version first")]
    NothingToShare,
    #[error(
        "someone else has shared changes since you last got updates; get updates first, then share again"
    )]
    ShareRejected,
    #[error(
        "the shared location did not accept your sign-in; check that you have access, and that git is set up with your credentials or ssh key"
    )]
    AuthFailed,
    #[error("the shared location could not be reached; check the address and your connection")]
    RemoteUnreachable,
    #[error(
        "your project and the shared copy do not share any history, so they cannot be combined"
    )]
    UnrelatedHistories,
    #[error("git backend: {0}")]
    Backend(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl GitError {
    pub(crate) fn backend(e: impl std::fmt::Display) -> Self {
        Self::Backend(e.to_string())
    }

    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Stable identifier for UI branching.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoRoot => "no-root",
            Self::NotARepo(_) => "not-a-repo",
            Self::AlreadyRepo(_) => "already-a-repo",
            Self::Path(_) => "bad-path",
            Self::BadArgs(_) => "bad-arguments",
            Self::ConfirmRequired => "confirm-required",
            Self::MissingIdentity => "missing-identity",
            Self::NothingToCommit => "nothing-to-commit",
            Self::EmptyMessage => "empty-message",
            Self::AmendPublished => "amend-published",
            Self::AmendUnborn => "amend-unborn",
            Self::DirtyTree(_) | Self::DirtyPaths(_) => "dirty-tree",
            Self::NothingToSave => "nothing-to-save",
            Self::BadBranchName(_) => "bad-branch-name",
            Self::GitMissing => "git-missing",
            Self::Timeout { .. } => "timeout",
            Self::TooLarge { .. } => "too-large",
            Self::CommandFailed { .. } => "command-failed",
            Self::RevNotFound(_) => "rev-not-found",
            Self::PathNotInRev(_) => "path-not-in-rev",
            Self::NoRemote => "no-remote",
            Self::BadRemoteUrl(_) => "bad-remote-url",
            Self::NothingToShare => "nothing-to-share",
            Self::ShareRejected => "share-rejected",
            Self::AuthFailed => "auth-failed",
            Self::RemoteUnreachable => "remote-unreachable",
            Self::UnrelatedHistories => "unrelated-histories",
            Self::Backend(_) => "backend",
            Self::Io { .. } => "io",
        }
    }
}

impl From<GitError> for PluginError {
    fn from(e: GitError) -> Self {
        let mut body = json!({ "code": e.code(), "message": e.to_string() });
        if let GitError::DirtyPaths(paths) = &e {
            body["paths"] = json!(paths);
        }
        PluginError::Failed(body.to_string())
    }
}
