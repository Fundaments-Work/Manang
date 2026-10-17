use std::path::PathBuf;

use manang_kernel::PluginError;
use thiserror::Error;

/// Why a writing-tools request could not be served.
#[derive(Debug, Error)]
pub enum WritingError {
    #[error("invalid argument `{field}`: {reason}")]
    BadArgument { field: &'static str, reason: String },
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("syntax error: {0}")]
    Syntax(String),
    #[error("no dictionary for language `{0}`")]
    NoDictionary(String),
    #[error("dictionary `{language}` is unusable: {reason}")]
    BadDictionary { language: String, reason: String },
    #[error("cannot access `{}`: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("range {from}..{to} is outside the text ({len} characters)")]
    BadRange { from: usize, to: usize, len: usize },
    #[error("invalid snippet: {0}")]
    Snippet(String),
}

impl WritingError {
    pub(crate) fn bad_arg(field: &'static str, reason: impl Into<String>) -> Self {
        Self::BadArgument {
            field,
            reason: reason.into(),
        }
    }
}

impl From<WritingError> for PluginError {
    fn from(error: WritingError) -> Self {
        PluginError::Failed(error.to_string())
    }
}
