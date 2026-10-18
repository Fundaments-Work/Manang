//! Helpers adapters share to plan text edits. Pure functions on `&str` and
//! byte ranges: no state, no allocation beyond the plan.

use std::ops::Range;

use crate::adapter::{EditPlan, TextEdit};

/// A construct already wrapping the selection: `outer` is the whole span, its
/// opening and closing delimiters are the first `open_len` and last
/// `close_len` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enclosing {
    pub outer: Range<usize>,
    pub open_len: usize,
    pub close_len: usize,
}

impl Enclosing {
    /// Whether `sel` lies in the content of `outer` or covers all of it.
    pub fn holds(&self, sel: &Range<usize>) -> bool {
        let content =
            self.outer.start + self.open_len..self.outer.end.saturating_sub(self.close_len);
        let inside = sel.start >= content.start && sel.end <= content.end;
        let covers = sel.start <= self.outer.start && sel.end >= self.outer.end;
        inside || covers
    }
}

/// Clamp `r` into `text` and onto char boundaries (moving inwards).
pub fn clamp(text: &str, r: Range<usize>) -> Range<usize> {
    let mut start = r.start.min(text.len());
    let mut end = r.end.clamp(start, text.len());
    while !text.is_char_boundary(start) {
        start += 1;
    }
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    start..end.max(start)
}

/// Start of the line containing `pos`.
pub fn line_start(text: &str, pos: usize) -> usize {
    let pos = pos.min(text.len());
    text[..pos].rfind('\n').map_or(0, |i| i + 1)
}

/// End of the line containing `pos`, before its `\n`.
pub fn line_end(text: &str, pos: usize) -> usize {
    let pos = pos.min(text.len());
    text[pos..].find('\n').map_or(text.len(), |i| pos + i)
}

/// The lines a selection touches, without their newlines. A selection that
/// ends exactly at the start of a line does not touch that line.
pub fn lines_of(text: &str, sel: &Range<usize>) -> Vec<Range<usize>> {
    let sel = clamp(text, sel.clone());
    let first = line_start(text, sel.start);
    let last_pos = if sel.end > sel.start && text.as_bytes().get(sel.end - 1) == Some(&b'\n') {
        sel.end - 1
    } else {
        sel.end
    };
    let mut out = Vec::new();
    let mut at = first;
    loop {
        let end = line_end(text, at);
        out.push(at..end);
        if end >= last_pos || end >= text.len() {
            break;
        }
        at = end + 1;
    }
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// The word around a caret, or the empty range when the caret is not in or
/// next to one. A non-empty selection is returned unchanged.
pub fn expand_word(text: &str, sel: Range<usize>) -> Range<usize> {
    let sel = clamp(text, sel);
    if sel.start != sel.end {
        return sel;
    }
    let before = text[..sel.start]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(sel.start, |(i, _)| i);
    let after = text[sel.start..]
        .char_indices()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(sel.start, |(i, c)| sel.start + i + c.len_utf8());
    before..after
}

/// `sel` without leading and trailing whitespace.
pub fn trim_ws(text: &str, sel: Range<usize>) -> Range<usize> {
    let sel = clamp(text, sel);
    let slice = &text[sel.clone()];
    let lead = slice.len() - slice.trim_start().len();
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return sel.start..sel.start;
    }
    sel.start + lead..sel.start + lead + trimmed.len()
}

/// Toggle an inline wrapper (`**`, `*`, `` ` ``, `$`...) around the selection.
///
/// With an `enclosing` construct the delimiters are removed. Otherwise a
/// non-empty selection (trimmed of whitespace) or the word at the caret is
/// wrapped, and a caret in empty space gets an empty pair with the caret
/// between them.
pub fn toggle_wrap(
    text: &str,
    sel: Range<usize>,
    open: &str,
    close: &str,
    enclosing: Option<&Enclosing>,
) -> EditPlan {
    let sel = clamp(text, sel);
    if let Some(e) = enclosing {
        let content_start = e.outer.start + e.open_len;
        let content_end = e.outer.end.saturating_sub(e.close_len);
        let map = |p: usize| -> usize {
            if p <= content_start {
                e.outer.start
            } else if p >= content_end {
                content_end - e.open_len
            } else {
                p - e.open_len
            }
        };
        return EditPlan {
            edits: vec![
                TextEdit::delete(e.outer.start..content_start),
                TextEdit::delete(content_end..e.outer.end),
            ],
            selection: Some(map(sel.start)..map(sel.end)),
        };
    }
    let target = trim_ws(text, expand_word(text, sel.clone()));
    if target.start == target.end {
        let at = sel.start;
        return EditPlan {
            edits: vec![TextEdit::insert(at, format!("{open}{close}"))],
            selection: Some(at + open.len()..at + open.len()),
        };
    }
    EditPlan {
        edits: vec![
            TextEdit::insert(target.start, open),
            TextEdit::insert(target.end, close),
        ],
        selection: Some(target.start + open.len()..target.end + open.len()),
    }
}

/// Replace the selection (or insert at the caret) with `build(selected_text)`,
/// and select the result's `inner` part afterwards.
pub fn replace_selection(
    text: &str,
    sel: Range<usize>,
    replacement: String,
    caret_in_replacement: Range<usize>,
) -> EditPlan {
    let sel = clamp(text, sel);
    let base = sel.start;
    EditPlan {
        edits: vec![TextEdit::replace(sel, replacement)],
        selection: Some(base + caret_in_replacement.start..base + caret_in_replacement.end),
    }
}

/// Map a position through sorted, non-overlapping edits (pre to post coordinates).
pub fn map_through(edits: &[TextEdit], pos: usize) -> usize {
    let mut delta: isize = 0;
    for e in edits {
        if e.range.end <= pos {
            delta += e.replacement.len() as isize - (e.range.end - e.range.start) as isize;
        } else if e.range.start < pos {
            // Inside a replaced range: snap to the end of the replacement.
            return (e.range.start as isize + delta) as usize + e.replacement.len();
        } else {
            break;
        }
    }
    (pos as isize + delta).max(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, plan: &EditPlan) -> String {
        let mut out = text.to_owned();
        for e in plan.edits.iter().rev() {
            out.replace_range(e.range.clone(), &e.replacement);
        }
        out
    }

    #[test]
    fn wraps_selection_word_and_empty() {
        let t = "say hello there";
        let p = toggle_wrap(t, 4..9, "**", "**", None);
        assert_eq!(apply(t, &p), "say **hello** there");
        assert_eq!(p.selection, Some(6..11));
        let p = toggle_wrap(t, 6..6, "*", "*", None);
        assert_eq!(apply(t, &p), "say *hello* there");
        let p = toggle_wrap("a  b", 2..2, "_", "_", None);
        assert_eq!(apply("a  b", &p), "a __ b");
        assert_eq!(p.selection, Some(3..3));
    }

    #[test]
    fn trims_whitespace_inside_the_selection() {
        let t = "x  word  y";
        let p = toggle_wrap(t, 1..9, "*", "*", None);
        assert_eq!(apply(t, &p), "x  *word*  y");
    }

    #[test]
    fn unwraps_when_enclosed() {
        let t = "a **bold** b";
        let e = Enclosing {
            outer: 2..10,
            open_len: 2,
            close_len: 2,
        };
        assert!(e.holds(&(6..6)));
        let p = toggle_wrap(t, 4..8, "**", "**", Some(&e));
        assert_eq!(apply(t, &p), "a bold b");
        assert_eq!(p.selection, Some(2..6));
    }

    #[test]
    fn lines_of_a_selection() {
        let t = "ab\ncd\nef";
        assert_eq!(lines_of(t, &(0..0)), vec![0..2]);
        assert_eq!(lines_of(t, &(1..4)), vec![0..2, 3..5]);
        assert_eq!(lines_of(t, &(0..3)), vec![0..2]);
        assert_eq!(lines_of(t, &(0..8)), vec![0..2, 3..5, 6..8]);
    }

    #[test]
    fn expand_word_handles_unicode() {
        let t = "né ça";
        assert_eq!(expand_word(t, 1..1), 0..3);
        assert_eq!(expand_word(t, 3..3), 0..3);
    }

    #[test]
    fn mapping_through_edits() {
        let edits = vec![TextEdit::insert(2, "**"), TextEdit::insert(5, "**")];
        assert_eq!(map_through(&edits, 1), 1);
        assert_eq!(map_through(&edits, 3), 5);
        assert_eq!(map_through(&edits, 9), 13);
    }
}
