//! Error types for the figure plugin and scene store.

use thiserror::Error;

/// Errors produced during figure operations.
#[derive(Debug, Error)]
pub enum FigureError {
    /// No figure is currently active.
    #[error("no figure is currently active")]
    NoActiveFigure,

    /// The requested figure ID does not exist in the store.
    #[error("no figure with id {0}")]
    NoSuchFigure(u64),

    /// The figure does not have a filesystem path associated with it.
    #[error("figure has no file path associated")]
    NoPath,

    /// The target node was not found in the scene.
    #[error("node with id `{0}` not found")]
    NodeNotFound(String),

    /// A node with the specified ID already exists in the scene.
    #[error("node with id `{0}` already exists")]
    DuplicateNode(String),

    /// An I/O error occurred while reading or writing a figure file.
    #[error("I/O error at `{path}`: {source}")]
    Io {
        /// File path where the error occurred.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// An error occurred during JSON serialization or deserialization.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// An invalid operation or argument was provided.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}
