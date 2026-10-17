use ropey::Rope;
use std::ops::{Bound, Range, RangeBounds};

use crate::error::BufferError;
use crate::history::{Edit, History};
use crate::position::Position;

/// A high-performance, Rope-based document buffer with transaction history.
#[derive(Debug, Clone)]
pub struct DocumentBuffer {
    rope: Rope,
    history: History,
}

impl Default for DocumentBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentBuffer {
    /// Creates a new, empty document buffer.
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            history: History::default(),
        }
    }

    /// Opens and initializes a document buffer from a string slice.
    pub fn open(initial_text: &str) -> Self {
        Self {
            rope: Rope::from_str(initial_text),
            history: History::default(),
        }
    }

    /// Returns the total character count (Unicode scalar values) of the buffer.
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Returns the total UTF-8 byte length of the buffer.
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Returns true if the buffer contains zero characters.
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// Returns the total line count in the document.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Returns the character length of the specified line (including any trailing newline).
    pub fn line_len_chars(&self, line: usize) -> Result<usize, BufferError> {
        let count = self.line_count();
        if line >= count {
            return Err(BufferError::LineOutOfBounds {
                line,
                line_count: count,
            });
        }
        Ok(self.rope.line(line).len_chars())
    }

    /// Converts a character index to a zero-based (line, column) tuple.
    pub fn char_to_line_col(&self, char_idx: usize) -> Result<(usize, usize), BufferError> {
        let pos = self.char_to_position(char_idx)?;
        Ok((pos.line, pos.column))
    }

    /// Converts a zero-based (line, column) coordinate to a character index.
    pub fn line_col_to_char(&self, line: usize, col: usize) -> Result<usize, BufferError> {
        self.position_to_char(Position::new(line, col))
    }

    /// Converts a character index to a structured `Position`.
    pub fn char_to_position(&self, char_idx: usize) -> Result<Position, BufferError> {
        let len_chars = self.rope.len_chars();
        if char_idx > len_chars {
            return Err(BufferError::CharIndexOutOfBounds {
                char_idx,
                len_chars,
            });
        }
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let column = char_idx - line_start;
        Ok(Position::new(line, column))
    }

    /// Converts a structured `Position` to a character index.
    pub fn position_to_char(&self, pos: Position) -> Result<usize, BufferError> {
        let line_count = self.rope.len_lines();
        if pos.line >= line_count {
            return Err(BufferError::LineOutOfBounds {
                line: pos.line,
                line_count,
            });
        }
        let line_slice = self.rope.line(pos.line);
        let line_len = line_slice.len_chars();
        if pos.column > line_len {
            return Err(BufferError::ColumnOutOfBounds {
                line: pos.line,
                col: pos.column,
                line_len_chars: line_len,
            });
        }
        let line_start = self.rope.line_to_char(pos.line);
        Ok(line_start + pos.column)
    }

    /// Maps a character index to its corresponding UTF-8 byte offset.
    pub fn char_to_byte_idx(&self, char_idx: usize) -> Result<usize, BufferError> {
        let len_chars = self.rope.len_chars();
        if char_idx > len_chars {
            return Err(BufferError::CharIndexOutOfBounds {
                char_idx,
                len_chars,
            });
        }
        Ok(self.rope.char_to_byte(char_idx))
    }

    /// Maps a UTF-8 byte offset to its corresponding character index.
    ///
    /// Returns `Err(BufferError::InvalidUtf8Boundary)` if `byte_offset` falls within
    /// a multi-byte sequence rather than on a UTF-8 code point boundary.
    pub fn byte_to_char_idx(&self, byte_offset: usize) -> Result<usize, BufferError> {
        let len_bytes = self.rope.len_bytes();
        if byte_offset > len_bytes {
            return Err(BufferError::ByteOffsetOutOfBounds {
                byte_offset,
                len_bytes,
            });
        }
        let char_idx = self.rope.byte_to_char(byte_offset);
        if self.rope.char_to_byte(char_idx) != byte_offset {
            return Err(BufferError::InvalidUtf8Boundary { byte_offset });
        }
        Ok(char_idx)
    }

    /// Resolves an arbitrary range bound to a concrete, validated `Range<usize>` in character units.
    pub fn resolve_range<R: RangeBounds<usize>>(
        &self,
        range: R,
    ) -> Result<Range<usize>, BufferError> {
        let len = self.rope.len_chars();
        let start = match range.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s.checked_add(1).ok_or(BufferError::CharIndexOutOfBounds {
                char_idx: usize::MAX,
                len_chars: len,
            })?,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&e) => e.checked_add(1).ok_or(BufferError::CharIndexOutOfBounds {
                char_idx: usize::MAX,
                len_chars: len,
            })?,
            Bound::Excluded(&e) => e,
            Bound::Unbounded => len,
        };

        if start > end {
            return Err(BufferError::InvalidRange { start, end });
        }
        if end > len {
            return Err(BufferError::CharIndexOutOfBounds {
                char_idx: end,
                len_chars: len,
            });
        }
        Ok(start..end)
    }

    /// Returns a string slice of the specified character range.
    pub fn slice<R: RangeBounds<usize>>(&self, range: R) -> Result<String, BufferError> {
        let resolved = self.resolve_range(range)?;
        Ok(self.rope.slice(resolved).to_string())
    }

    /// Inserts text at the specified character index.
    pub fn insert(&mut self, char_idx: usize, text: &str) -> Result<(), BufferError> {
        let len = self.rope.len_chars();
        if char_idx > len {
            return Err(BufferError::CharIndexOutOfBounds {
                char_idx,
                len_chars: len,
            });
        }
        if text.is_empty() {
            return Ok(());
        }

        self.rope.insert(char_idx, text);
        self.history.record_edit(Edit::Insert {
            char_idx,
            text: text.to_owned(),
        });
        Ok(())
    }

    /// Deletes text across the specified character range, returning the deleted text.
    pub fn delete<R: RangeBounds<usize>>(&mut self, range: R) -> Result<String, BufferError> {
        let resolved = self.resolve_range(range)?;
        if resolved.is_empty() {
            return Ok(String::new());
        }

        let deleted = self.rope.slice(resolved.clone()).to_string();
        self.rope.remove(resolved.clone());
        self.history.record_edit(Edit::Delete {
            char_idx: resolved.start,
            text: deleted.clone(),
        });
        Ok(deleted)
    }

    /// Replaces the text in `range` with `text`, returning the replaced text.
    ///
    /// If executed outside an explicit transaction, this replacement is recorded as a single
    /// atomic undo/redo step.
    pub fn replace<R: RangeBounds<usize>>(
        &mut self,
        range: R,
        text: &str,
    ) -> Result<String, BufferError> {
        let resolved = self.resolve_range(range)?;
        if resolved.is_empty() && text.is_empty() {
            return Ok(String::new());
        }

        let was_in_tx = self.history.is_in_transaction();
        if !was_in_tx {
            self.history.begin_transaction();
        }

        let deleted = self.delete(resolved.clone())?;
        if !text.is_empty() {
            self.insert(resolved.start, text)?;
        }

        if !was_in_tx {
            let _ = self.history.commit_transaction();
        }

        Ok(deleted)
    }

    /// Begins a new transaction for grouping multiple edits into an atomic undo/redo step.
    /// Transactions support nesting; only the outermost commit records the transaction.
    pub fn begin_transaction(&mut self) {
        self.history.begin_transaction();
    }

    /// Commits the active transaction.
    pub fn commit_transaction(&mut self) -> Result<(), BufferError> {
        self.history.commit_transaction().map(|_| ())
    }

    /// Rolls back the active transaction, reverting all edits made since `begin_transaction`.
    pub fn rollback_transaction(&mut self) -> Result<(), BufferError> {
        let tx = self.history.rollback_transaction()?;

        for edit in tx.edits.into_iter().rev() {
            match edit.inverted() {
                Edit::Insert { char_idx, text } => {
                    self.rope.insert(char_idx, &text);
                }
                Edit::Delete { char_idx, text } => {
                    let end = char_idx + text.chars().count();
                    self.rope.remove(char_idx..end);
                }
            }
        }
        Ok(())
    }

    /// Executes a closure inside an atomic transaction, rolling back automatically on error.
    pub fn transaction<T, F>(&mut self, f: F) -> Result<T, BufferError>
    where
        F: FnOnce(&mut DocumentBuffer) -> Result<T, BufferError>,
    {
        self.begin_transaction();
        match f(self) {
            Ok(value) => {
                self.commit_transaction()?;
                Ok(value)
            }
            Err(err) => {
                let _ = self.rollback_transaction();
                Err(err)
            }
        }
    }

    /// Reverts the latest transaction from the undo stack.
    /// Returns `Ok(true)` if an undo was performed, `Ok(false)` if the undo stack was empty.
    pub fn undo(&mut self) -> Result<bool, BufferError> {
        if self.history.is_in_transaction() {
            return Err(BufferError::TransactionInProgress);
        }
        let Some(tx) = self.history.pop_undo() else {
            return Ok(false);
        };

        for edit in tx.edits.into_iter().rev() {
            match edit.inverted() {
                Edit::Insert { char_idx, text } => {
                    self.rope.insert(char_idx, &text);
                }
                Edit::Delete { char_idx, text } => {
                    let end = char_idx + text.chars().count();
                    self.rope.remove(char_idx..end);
                }
            }
        }
        Ok(true)
    }

    /// Re-applies the latest undone transaction from the redo stack.
    /// Returns `Ok(true)` if a redo was performed, `Ok(false)` if the redo stack was empty.
    pub fn redo(&mut self) -> Result<bool, BufferError> {
        if self.history.is_in_transaction() {
            return Err(BufferError::TransactionInProgress);
        }
        let Some(tx) = self.history.pop_redo() else {
            return Ok(false);
        };

        for edit in tx.edits {
            match edit {
                Edit::Insert { char_idx, text } => {
                    self.rope.insert(char_idx, &text);
                }
                Edit::Delete { char_idx, text } => {
                    let end = char_idx + text.chars().count();
                    self.rope.remove(char_idx..end);
                }
            }
        }
        Ok(true)
    }

    /// Whether an undo operation is currently possible.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether a redo operation is currently possible.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Clears the entire undo and redo history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}

impl std::fmt::Display for DocumentBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for chunk in self.rope.chunks() {
            f.write_str(chunk)?;
        }
        Ok(())
    }
}
