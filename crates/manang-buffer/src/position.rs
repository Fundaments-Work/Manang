use serde::{Deserialize, Serialize};

/// A zero-based line and column position within a document buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Position {
    /// Zero-based line index.
    pub line: usize,
    /// Zero-based character column index within the line.
    pub column: usize,
}

impl Position {
    /// Creates a new `Position` with zero-based `line` and `column`.
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}
