//! Errors of the agent core.

use thiserror::Error;

/// Everything this crate can fail at. No variant ever carries a secret.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A content block was not a JSON object with a string `type`.
    #[error("invalid content block: {0}")]
    InvalidBlock(String),
    /// JSON (de)serialisation failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// A transcript or audit line failed to parse.
    #[error("line {line}: {message}")]
    BadLine { line: usize, message: String },
    /// Writing to a sink failed.
    #[error("write failed: {0}")]
    Io(#[from] std::io::Error),
    /// A request is malformed (too many cache breakpoints, no model...).
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// A tool name cannot be mapped to a provider-safe name.
    #[error("tool name `{name}`: {reason}")]
    ToolName { name: String, reason: String },
    /// Policy forbids the operation.
    #[error("policy: {0}")]
    Policy(String),
}
