//! Error types for the project plugin.

use std::fmt;
use std::io;

use thiserror::Error;

/// One problem found while validating `manang.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Dotted key the problem is about, e.g. `project.main`.
    pub key: String,
    pub message: String,
}

impl Issue {
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`: {}", self.key, self.message)
    }
}

/// `manang.toml` could not be turned into a valid [`crate::ProjectConfig`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// Not TOML at all.
    #[error("manang.toml is not valid TOML:\n{0}")]
    Syntax(String),
    /// Valid TOML but the wrong shape (missing key, wrong type, bad value).
    #[error("manang.toml does not match the schema:\n{0}")]
    Schema(String),
    /// Well-formed but semantically invalid; every problem is listed.
    #[error("manang.toml is invalid:{}", list(.0))]
    Invalid(Vec<Issue>),
}

fn list(issues: &[Issue]) -> String {
    issues.iter().map(|i| format!("\n  - {i}")).collect()
}

/// Everything the project plugin can fail with.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("path `{0}` escapes the project root")]
    Escapes(String),
    #[error("invalid path `{path}`: {reason}")]
    InvalidPath { path: String, reason: &'static str },
    #[error("`{0}` is protected and cannot be modified")]
    Protected(String),
    #[error("`{0}` is not valid UTF-8 text")]
    NotUtf8(String),
    #[error("`{path}` is {size} bytes, over the {limit} byte limit")]
    TooLarge { path: String, size: u64, limit: u64 },
    #[error("no project is open")]
    NoProject,
    #[error("`{0}` already exists")]
    Exists(String),
    #[error("`{0}` does not exist")]
    NotFound(String),
    #[error("`{0}` is not a Manang project (no manang.toml)")]
    NotAProject(String),
    #[error("`{0}` is not a directory")]
    NotADirectory(String),
    #[error("`{0}` is not a regular file")]
    NotAFile(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("git: {0}")]
    Git(String),
    #[error("file watcher: {0}")]
    Watch(String),
}

impl ProjectError {
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
