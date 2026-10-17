//! Character-offset bookkeeping.
//!
//! The editor addresses text by Unicode scalar index; Rust parsers report byte
//! offsets. [`TextIndex`] converts between them in O(log n + 256) using sparse
//! checkpoints, so thousands of diagnostics on a large document stay cheap.

use serde::Serialize;

const CHECKPOINT_BYTES: usize = 256;

/// A half-open range of Unicode scalar values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CharRange {
    pub start: usize,
    pub end: usize,
}

pub struct TextIndex<'a> {
    text: &'a str,
    /// `(byte, char)` pairs, ascending, always starting with `(0, 0)`.
    checkpoints: Vec<(usize, usize)>,
    /// Character offset of the start of every line.
    line_starts: Vec<usize>,
    chars: usize,
}

impl<'a> TextIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut checkpoints = vec![(0, 0)];
        let mut line_starts = vec![0];
        let mut next = CHECKPOINT_BYTES;
        let mut chars = 0;
        for (byte, ch) in text.char_indices() {
            if byte >= next {
                checkpoints.push((byte, chars));
                next = byte + CHECKPOINT_BYTES;
            }
            chars += 1;
            if ch == '\n' {
                line_starts.push(chars);
            }
        }
        Self {
            text,
            checkpoints,
            line_starts,
            chars,
        }
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Number of characters in the text.
    pub fn char_len(&self) -> usize {
        self.chars
    }

    /// Character offset of a byte offset. A byte inside a character maps to
    /// that character; one past the end maps to the character count.
    pub fn byte_to_char(&self, byte: usize) -> usize {
        let mut byte = byte.min(self.text.len());
        while !self.text.is_char_boundary(byte) {
            byte -= 1;
        }
        let at = self.checkpoints.partition_point(|&(b, _)| b <= byte);
        let (cb, cc) = self.checkpoints[at.saturating_sub(1)];
        cc + self.text[cb..byte].chars().count()
    }

    /// Byte offset of a character offset, clamped to the text length.
    pub fn char_to_byte(&self, chars: usize) -> usize {
        let chars = chars.min(self.chars);
        let at = self.checkpoints.partition_point(|&(_, c)| c <= chars);
        let (cb, cc) = self.checkpoints[at.saturating_sub(1)];
        match self.text[cb..].char_indices().nth(chars - cc) {
            Some((offset, _)) => cb + offset,
            None => self.text.len(),
        }
    }

    pub fn char_range(&self, bytes: std::ops::Range<usize>) -> CharRange {
        let start = self.byte_to_char(bytes.start);
        let end = self.byte_to_char(bytes.end).max(start);
        CharRange { start, end }
    }

    /// 1-based line and column (in characters) of a character offset.
    pub fn line_col(&self, chars: usize) -> (usize, usize) {
        let chars = chars.min(self.chars);
        let line = self.line_starts.partition_point(|&s| s <= chars) - 1;
        (line + 1, chars - self.line_starts[line] + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_both_ways() {
        let text = "aé\nβγ😀x";
        let idx = TextIndex::new(text);
        assert_eq!(idx.char_len(), 7);
        for (c, (b, _)) in text.char_indices().enumerate() {
            assert_eq!(idx.byte_to_char(b), c);
            assert_eq!(idx.char_to_byte(c), b);
        }
        assert_eq!(idx.byte_to_char(text.len()), 7);
        assert_eq!(idx.char_to_byte(99), text.len());
        // A byte inside the two-byte `é` maps to it.
        assert_eq!(idx.byte_to_char(2), 1);
    }

    #[test]
    fn long_text_checkpoints() {
        let text: String = "héllo wörld ".repeat(500);
        let idx = TextIndex::new(&text);
        for (c, (b, _)) in text.char_indices().enumerate().step_by(37) {
            assert_eq!(idx.byte_to_char(b), c);
            assert_eq!(idx.char_to_byte(c), b);
        }
    }

    #[test]
    fn lines_and_columns() {
        let idx = TextIndex::new("ab\ncd\n\nx");
        assert_eq!(idx.line_col(0), (1, 1));
        assert_eq!(idx.line_col(1), (1, 2));
        assert_eq!(idx.line_col(3), (2, 1));
        assert_eq!(idx.line_col(6), (3, 1));
        assert_eq!(idx.line_col(7), (4, 1));
        assert_eq!(idx.line_col(100), (4, 2));
    }
}
