//! Offsets and masking shared by the rules.
//!
//! Rules work in byte offsets of the UTF-8 text (what `str` slicing needs);
//! the engine converts to UTF-16 once, at the edge.

/// Line starts of a text, in bytes.
#[derive(Debug, Clone)]
pub struct Lines {
    starts: Vec<usize>,
    len: usize,
}

impl Lines {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            starts,
            len: text.len(),
        }
    }

    /// Number of lines (a trailing newline starts an empty last line).
    pub fn count(&self) -> usize {
        self.starts.len()
    }

    /// Byte range of 1-based `line`, without its newline.
    pub fn range(&self, text: &str, line: usize) -> Option<(usize, usize)> {
        let start = *self.starts.get(line.checked_sub(1)?)?;
        let end = self
            .starts
            .get(line)
            .map_or(self.len, |next| next.saturating_sub(1));
        let end = if text[start..end].ends_with('\r') {
            end - 1
        } else {
            end
        };
        Some((start, end.max(start)))
    }

    /// 1-based line of a byte offset.
    pub fn line_of(&self, byte: usize) -> usize {
        self.starts.partition_point(|s| *s <= byte)
    }
}

/// Byte offset of the `n`th Unicode scalar value (clamped to the end).
pub fn byte_of_scalar(text: &str, n: usize) -> usize {
    text.char_indices().nth(n).map_or(text.len(), |(i, _)| i)
}

/// Byte offset of 1-based `line`, 1-based scalar `column`.
pub fn byte_of_position(text: &str, lines: &Lines, line: usize, column: usize) -> Option<usize> {
    let (start, end) = lines.range(text, line)?;
    let offset = text[start..end]
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(end, |(i, _)| start + i);
    Some(offset)
}

/// UTF-16 offset of a byte offset on a char boundary.
pub fn utf16_of_byte(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].chars().map(char::len_utf16).sum()
}

/// The line ending the document uses, so inserted lines match.
pub fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Blank `range` with spaces, keeping newlines, so every offset stays valid.
pub fn blank(bytes: &mut [u8], from: usize, to: usize) {
    for b in bytes.iter_mut().take(to).skip(from) {
        if *b != b'\n' && *b != b'\r' {
            *b = b' ';
        }
    }
}

pub fn into_string(bytes: Vec<u8>) -> String {
    // Blanking only writes ASCII spaces over whole bytes of a valid string, but
    // a multi-byte character cut in half would not be valid: fall back to a
    // lossy copy of the same length class rather than panicking.
    String::from_utf8(bytes).unwrap_or_else(|e| {
        let bytes = e.into_bytes();
        bytes
            .into_iter()
            .map(|b| if b.is_ascii() { char::from(b) } else { ' ' })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_offsets() {
        let text = "ab\r\ncé\n\nf";
        let lines = Lines::new(text);
        assert_eq!(lines.count(), 4);
        assert_eq!(lines.range(text, 1), Some((0, 2)));
        assert_eq!(lines.range(text, 2), Some((4, 7)));
        assert_eq!(lines.range(text, 4), Some((9, 10)));
        assert_eq!(lines.range(text, 5), None);
        assert_eq!(lines.line_of(5), 2);
        assert_eq!(byte_of_position(text, &lines, 2, 2), Some(5));
        assert_eq!(utf16_of_byte("a😀b", 5), 3);
        assert_eq!(line_ending(text), "\r\n");
    }
}
