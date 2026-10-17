//! In-document find (`search.in-text`).
//!
//! Works on a string the editor hands over and answers in Unicode-scalar
//! offsets, the unit the editor's rope uses, so the front end never converts
//! between UTF-8 and UTF-16.

use grep_matcher::Matcher;
use serde::Serialize;

use crate::error::SearchError;
use crate::limits::Limits;
use crate::pattern::{Mode, PatternSpec, compile};

/// One match inside the searched text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextMatch {
    /// Scalar offset of the first matched char.
    pub start: usize,
    /// Scalar offset one past the last matched char.
    pub end: usize,
    /// 1-based line of `start`.
    pub line: usize,
    /// 1-based scalar column of `start` within its line.
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InTextOutcome {
    pub matches: Vec<TextMatch>,
    /// More matches exist than `limit`.
    pub truncated: bool,
}

/// Converts ascending byte offsets into scalar offsets in one pass.
pub struct CharCursor<'a> {
    text: &'a str,
    byte: usize,
    chars: usize,
    line: usize,
    line_start_chars: usize,
}

impl<'a> CharCursor<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            byte: 0,
            chars: 0,
            line: 1,
            line_start_chars: 0,
        }
    }

    /// Advance to `byte` (must not move backwards). `None` if `byte` is not
    /// on a char boundary or is out of range.
    pub fn advance(&mut self, byte: usize) -> Option<usize> {
        if byte < self.byte {
            return None;
        }
        let seg = self.text.get(self.byte..byte)?;
        for c in seg.chars() {
            self.chars += 1;
            if c == '\n' {
                self.line += 1;
                self.line_start_chars = self.chars;
            }
        }
        self.byte = byte;
        Some(self.chars)
    }

    /// 1-based line of the last offset advanced to.
    pub fn line(&self) -> usize {
        self.line
    }

    /// Scalar offset where the current line starts.
    pub fn line_start(&self) -> usize {
        self.line_start_chars
    }
}

/// Find every match of `spec` in `text`, at most `limit`.
pub fn find(
    text: &str,
    spec: &PatternSpec<'_>,
    limit: usize,
    limits: &Limits,
) -> Result<InTextOutcome, SearchError> {
    let matcher = compile(spec, Mode::Document, limits)?;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut truncated = false;
    // The matcher's error type is `NoError`; there is nothing to propagate.
    let _ = matcher.find_iter(text.as_bytes(), |m| {
        if m.is_empty() {
            return true;
        }
        if ranges.len() >= limit {
            truncated = true;
            return false;
        }
        ranges.push((m.start(), m.end()));
        true
    });

    let mut cursor = CharCursor::new(text);
    let mut matches = Vec::with_capacity(ranges.len());
    for (s, e) in ranges {
        // Matches ascend; a non-boundary offset (only reachable through
        // `(?-u)` byte patterns) is dropped instead of panicking.
        let Some(start) = cursor.advance(s) else {
            continue;
        };
        let (line, column) = (cursor.line(), start - cursor.line_start() + 1);
        let Some(end_chars) = text.get(s..e).map(|seg| seg.chars().count()) else {
            continue;
        };
        matches.push(TextMatch {
            start,
            end: start + end_chars,
            line,
            column,
        });
    }
    Ok(InTextOutcome { matches, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(q: &str) -> PatternSpec<'_> {
        PatternSpec {
            query: q,
            regex: false,
            case_sensitive: false,
            whole_word: false,
        }
    }

    fn run(text: &str, s: &PatternSpec<'_>) -> Vec<(usize, usize)> {
        find(text, s, 1000, &Limits::default())
            .expect("find")
            .matches
            .iter()
            .map(|m| (m.start, m.end))
            .collect()
    }

    #[test]
    fn literal_case_insensitive() {
        assert_eq!(
            run("Foo foo FOO", &spec("foo")),
            vec![(0, 3), (4, 7), (8, 11)]
        );
    }

    #[test]
    fn case_sensitive() {
        let mut s = spec("foo");
        s.case_sensitive = true;
        assert_eq!(run("Foo foo FOO", &s), vec![(4, 7)]);
    }

    #[test]
    fn whole_word() {
        let mut s = spec("cat");
        s.whole_word = true;
        assert_eq!(run("cat concat cat.", &s), vec![(0, 3), (11, 14)]);
    }

    #[test]
    fn regex_with_anchors() {
        let mut s = spec("^b+$");
        s.regex = true;
        assert_eq!(run("a\nbb\nc", &s), vec![(2, 4)]);
    }

    #[test]
    fn offsets_are_scalars_with_emoji_and_combining() {
        // 😀 is one scalar (4 bytes, 2 UTF-16 units); "e\u{301}" is two scalars.
        let text = "😀 e\u{301} 😀x";
        let got = find(text, &spec("x"), 10, &Limits::default()).expect("find");
        assert_eq!(got.matches.len(), 1);
        assert_eq!((got.matches[0].start, got.matches[0].end), (6, 7));
        assert_eq!(got.matches[0].column, 7);
        let got = run(text, &spec("\u{301}"));
        assert_eq!(got, vec![(3, 4)]);
    }

    #[test]
    fn line_and_column() {
        let got = find("ab\ncd\nxcd", &spec("cd"), 10, &Limits::default()).expect("find");
        let lc: Vec<_> = got.matches.iter().map(|m| (m.line, m.column)).collect();
        assert_eq!(lc, vec![(2, 1), (3, 2)]);
    }

    #[test]
    fn multiline_regex_spans_lines() {
        let mut s = spec(r"a\nb");
        s.regex = true;
        assert_eq!(run("xa\nbx", &s), vec![(1, 4)]);
    }

    #[test]
    fn limit_truncates() {
        let got = find("aaaa", &spec("a"), 2, &Limits::default()).expect("find");
        assert_eq!(got.matches.len(), 2);
        assert!(got.truncated);
    }

    #[test]
    fn empty_matches_are_skipped() {
        let mut s = spec("x*");
        s.regex = true;
        assert_eq!(run("axxb", &s), vec![(1, 3)]);
    }

    #[test]
    fn bad_pattern_is_an_error() {
        let mut s = spec("(");
        s.regex = true;
        assert!(find("x", &s, 10, &Limits::default()).is_err());
    }
}
