use crate::models::ParseDiagnostic;

#[derive(Debug, thiserror::Error)]
pub enum CitationsError {
    #[error("entry `{0}` not found")]
    EntryNotFound(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(#[from] ParseDiagnostic),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("unsupported syntax `{0}`: expected 'typst', 'latex', or 'markdown'")]
    UnsupportedSyntax(String),

    #[error("unsupported citation style `{0}`: expected 'apa' or 'numeric'")]
    UnsupportedStyle(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
