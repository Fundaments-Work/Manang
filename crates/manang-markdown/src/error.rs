//! Structured error types for markdown parsing and conversion.

use thiserror::Error;

/// Errors produced during markdown processing.
#[derive(Debug, Error)]
pub enum MarkdownError {
    /// Frontmatter failed to parse.
    #[error("frontmatter error: {0}")]
    Frontmatter(String),

    /// HTML rendering failed.
    #[error("render error: {0}")]
    Render(String),

    /// Typst conversion failed.
    #[error("typst conversion error: {0}")]
    TypstConversion(String),

    /// An invalid argument was passed to a command.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}
