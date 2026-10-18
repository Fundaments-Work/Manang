//! Structured errors for diagram rendering.
//!
//! The `Display` strings are a contract with the frontend: every message
//! starts with a bracketed machine-readable tag.

use thiserror::Error;

/// Errors produced while checking or rendering a diagram.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DiagramError {
    /// No engine handles the requested language id.
    #[error("[unknown_language] no diagram engine for `{0}`")]
    UnknownLanguage(String),

    /// The requested theme is not one of `dark`, `light`, `print`.
    #[error("[invalid_argument] unknown theme `{0}`; expected dark, light or print")]
    UnknownTheme(String),

    /// The source does not parse. Line and column are 1-based, the column
    /// counted in Unicode scalar values.
    #[error("[diagram_error] line {line}:{column}: {message}")]
    Syntax {
        /// 1-based line of the error.
        line: usize,
        /// 1-based column, in characters.
        column: usize,
        /// Human-readable description.
        message: String,
    },

    /// The source parsed but could not be laid out or emitted.
    #[error("[render_error] {0}")]
    Render(String),

    /// A command argument is missing or malformed.
    #[error("[invalid_argument] {0}")]
    InvalidArgument(String),
}
