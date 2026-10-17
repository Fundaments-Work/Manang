use thiserror::Error;

/// Errors that can occur during document buffer operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BufferError {
    /// The specified character index is out of bounds.
    #[error("char index {char_idx} is out of bounds (length: {len_chars})")]
    CharIndexOutOfBounds {
        /// The requested char index.
        char_idx: usize,
        /// The total number of chars in the buffer.
        len_chars: usize,
    },

    /// The specified byte offset is out of bounds.
    #[error("byte offset {byte_offset} is out of bounds (length: {len_bytes})")]
    ByteOffsetOutOfBounds {
        /// The requested byte offset.
        byte_offset: usize,
        /// The total byte length of the buffer.
        len_bytes: usize,
    },

    /// The specified byte offset does not land on a valid UTF-8 character boundary.
    #[error("byte offset {byte_offset} does not fall on a valid UTF-8 character boundary")]
    InvalidUtf8Boundary {
        /// The invalid byte offset.
        byte_offset: usize,
    },

    /// The character range has a start index greater than its end index.
    #[error("invalid character range: start {start} is greater than end {end}")]
    InvalidRange {
        /// The start of the range.
        start: usize,
        /// The end of the range.
        end: usize,
    },

    /// The requested line index is out of bounds.
    #[error("line {line} is out of bounds (total lines: {line_count})")]
    LineOutOfBounds {
        /// The requested line index.
        line: usize,
        /// Total lines in the document.
        line_count: usize,
    },

    /// The requested column index is out of bounds for the line.
    #[error("column {col} is out of bounds for line {line} (line length: {line_len_chars})")]
    ColumnOutOfBounds {
        /// The line index.
        line: usize,
        /// The requested column.
        col: usize,
        /// The character count of the line (including newline).
        line_len_chars: usize,
    },

    /// No active transaction is open to commit or rollback.
    #[error("no active transaction to commit or rollback")]
    NoActiveTransaction,

    /// An undo or redo was attempted while a transaction is actively open.
    #[error("cannot perform undo/redo while a transaction is in progress")]
    TransactionInProgress,
}
