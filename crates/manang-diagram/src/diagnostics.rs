//! Diagnostics in the editor's coordinate system.
//!
//! Engines report byte offsets into UTF-8 sources. The frontend's Problems
//! panel uses the convention of the Typst plugin: 1-based lines and 1-based
//! columns counted in Unicode scalar values (characters). End positions are
//! exclusive: `endColumn` is the column just past the last marked character.

use serde::Serialize;

use crate::error::DiagramError;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The diagram cannot be rendered.
    Error,
    /// The diagram renders but probably not as intended.
    Warning,
}

/// One problem found in a diagram source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// Error or warning.
    pub severity: Severity,
    /// Human-readable description.
    pub message: String,
    /// 1-based start line.
    pub line: usize,
    /// 1-based start column, in characters.
    pub column: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// 1-based exclusive end column, in characters.
    pub end_column: usize,
    /// Engine-specific code, when the engine provides one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// A 1-based line and character column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// 1-based line.
    pub line: usize,
    /// 1-based column, in characters.
    pub column: usize,
}

/// Clamp `byte` into `text` and down to the nearest character boundary, so a
/// stale or mid-character span never panics.
fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Converts a byte offset into a 1-based line and character column.
pub fn position(text: &str, byte: usize) -> Position {
    let byte = floor_boundary(text, byte);
    let before = text.get(..byte).unwrap_or_default();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = before.get(line_start..).unwrap_or_default().chars().count() + 1;
    Position { line, column }
}

impl Diagnostic {
    /// A diagnostic over the byte range `start..end` of `text`. An empty range
    /// is widened to the next character so the editor has something to mark.
    pub fn from_byte_span(
        text: &str,
        start: usize,
        end: usize,
        severity: Severity,
        message: impl Into<String>,
        code: Option<String>,
    ) -> Self {
        let start = floor_boundary(text, start);
        let mut end = floor_boundary(text, end).max(start);
        if end == start
            && let Some(ch) = text.get(start..).and_then(|rest| rest.chars().next())
            && ch != '\n'
        {
            end = start + ch.len_utf8();
        }
        let from = position(text, start);
        let to = position(text, end);
        Self {
            severity,
            message: message.into(),
            line: from.line,
            column: from.column,
            end_line: to.line,
            end_column: to.column,
            code,
        }
    }

    /// A diagnostic over the whole source, for errors without a location.
    pub fn whole_source(
        text: &str,
        severity: Severity,
        message: impl Into<String>,
        code: Option<String>,
    ) -> Self {
        Self::from_byte_span(text, 0, text.len(), severity, message, code)
    }

    /// The render error matching this diagnostic.
    pub fn to_error(&self) -> DiagramError {
        DiagramError::Syntax {
            line: self.line,
            column: self.column,
            message: self.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_characters_not_bytes() {
        let text = "ab\nçé|x";
        assert_eq!(position(text, 0), Position { line: 1, column: 1 });
        assert_eq!(position(text, 3), Position { line: 2, column: 1 });
        // "çé" is four bytes but two characters.
        assert_eq!(position(text, 7), Position { line: 2, column: 3 });
        // Mid-character offsets fall back to the boundary below.
        assert_eq!(position(text, 4), Position { line: 2, column: 1 });
        assert_eq!(position(text, 999), Position { line: 2, column: 5 });
    }

    #[test]
    fn empty_spans_widen_to_one_character() {
        let d = Diagnostic::from_byte_span("aé", 1, 1, Severity::Error, "m", None);
        assert_eq!((d.column, d.end_column), (2, 3));
    }

    #[test]
    fn serialises_with_camel_case_ends() {
        let d = Diagnostic::whole_source("a\nbc", Severity::Warning, "m", None);
        let v = serde_json::to_value(&d).unwrap_or_default();
        assert_eq!(v["endLine"], 2);
        assert_eq!(v["endColumn"], 3);
        assert_eq!(v["severity"], "warning");
        assert!(v.get("code").is_none());
    }
}
