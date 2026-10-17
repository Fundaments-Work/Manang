//! Diagnostic types for markdown rendering and conversion.

use serde::{Deserialize, Serialize};

/// A diagnostic message emitted during markdown processing or conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Severity level ("info", "warning", "error").
    pub severity: String,
    /// Human-readable description of the issue or unsupported construct.
    pub message: String,
    /// 1-based line number in source text if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Diagnostic {
    /// Creates a warning diagnostic.
    pub fn warning(message: impl Into<String>, line: Option<usize>) -> Self {
        Self {
            severity: "warning".to_string(),
            message: message.into(),
            line,
        }
    }

    /// Creates an informational diagnostic.
    pub fn info(message: impl Into<String>, line: Option<usize>) -> Self {
        Self {
            severity: "info".to_string(),
            message: message.into(),
            line,
        }
    }

    /// Creates an error diagnostic.
    pub fn error(message: impl Into<String>, line: Option<usize>) -> Self {
        Self {
            severity: "error".to_string(),
            message: message.into(),
            line,
        }
    }
}
