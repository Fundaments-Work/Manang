//! Unclosed and stray delimiters.
//!
//! Typst names the delimiter that was opened and never closed, so the rule
//! only has to decide where its closer goes. That is a judgement (a call's
//! arguments end where the author stopped), so these fixes are `likely`.

use crate::engine::{ByteEdit, Ctx, Draft};
use crate::text::line_ending;
use crate::types::Confidence;

use super::Placed;

const RULE_UNCLOSED: &str = "typst.unclosed-delimiter";
const RULE_STRAY: &str = "typst.stray-closer";

pub(super) fn unclosed(ctx: &Ctx<'_>, placed: &[Placed<'_>]) -> Vec<Draft> {
    let text = ctx.text;
    // Openers, outermost first.
    let mut openers: Vec<(&Placed<'_>, char)> = placed
        .iter()
        .filter(|p| matches!(p.diagnostic.message.as_str(), "unclosed delimiter"))
        .filter_map(|p| text[p.from..].chars().next().map(|c| (p, c)))
        .filter(|(_, c)| matches!(c, '(' | '[' | '{' | '$'))
        .collect();
    openers.sort_by_key(|(p, _)| p.from);
    let mut out = Vec::new();
    if let Some((first, _)) = openers.first() {
        let closers: String = openers
            .iter()
            .rev()
            .map(|(p, c)| closer_text(text, p.from, *c))
            .collect();
        let at = close_position(text, first.from, openers[0].1);
        let listed: Vec<String> = openers
            .iter()
            .rev()
            .map(|(_, c)| closer_name(*c).to_owned())
            .collect();
        let line = ctx.lines.line_of(first.from);
        out.push(Draft {
            id: format!("typst.close:{}:{at}", closers.replace(' ', "")),
            label: format!(
                "Add {} to close the `{}` on line {line}",
                listed.join(" "),
                openers[0].1
            ),
            why: "A delimiter is opened and never closed; this closes it where the \
                  statement appears to end."
                .to_owned(),
            edits: vec![ByteEdit::insert(at, closers)],
            confidence: Confidence::Likely,
            rule: RULE_UNCLOSED,
            diagnostics: openers.iter().map(|(p, _)| p.index).collect(),
            finding: None,
        });
    }
    for p in placed {
        match p.diagnostic.message.as_str() {
            "unclosed string" if text[p.from..].starts_with('"') => {
                let at = trim_comment_end(text, line_bounds(text, p.from).1);
                out.push(Draft {
                    id: format!("typst.close-string:{at}"),
                    label: "Add the closing quote".to_owned(),
                    why: "A string is opened and never closed; this closes it at the end of its line."
                        .to_owned(),
                    edits: vec![ByteEdit::insert(at, "\"")],
                    confidence: Confidence::Likely,
                    rule: RULE_UNCLOSED,
                    diagnostics: vec![p.index],
                    finding: None,
                });
            }
            "unclosed raw text" => {
                let ticks = text[p.from..].chars().take_while(|c| *c == '`').count();
                if ticks == 0 {
                    continue;
                }
                let fence = "`".repeat(ticks);
                let eol = line_ending(text);
                let (at, insert) = if ticks >= 3 {
                    let lead = if text.ends_with('\n') { "" } else { eol };
                    (text.len(), format!("{lead}{fence}{eol}"))
                } else {
                    (trim_comment_end(text, line_bounds(text, p.from).1), fence)
                };
                out.push(Draft {
                    id: format!("typst.close-raw:{ticks}:{at}"),
                    label: format!(
                        "Add the closing {}",
                        if ticks >= 3 { "fence" } else { "backtick" }
                    ),
                    why: "A raw block is opened and never closed.".to_owned(),
                    edits: vec![ByteEdit::insert(at, insert)],
                    confidence: Confidence::Likely,
                    rule: RULE_UNCLOSED,
                    diagnostics: vec![p.index],
                    finding: None,
                });
            }
            "unclosed block comment" => {
                let eol = line_ending(text);
                let lead = if text.ends_with('\n') { "" } else { eol };
                out.push(Draft {
                    id: "typst.close-comment".to_owned(),
                    label: "Add */ at the end".to_owned(),
                    why: "A block comment is opened and never closed.".to_owned(),
                    edits: vec![ByteEdit::insert(text.len(), format!("{lead}*/{eol}"))],
                    confidence: Confidence::Likely,
                    rule: RULE_UNCLOSED,
                    diagnostics: vec![p.index],
                    finding: None,
                });
            }
            _ => {}
        }
    }
    out
}

/// `unexpected closing bracket` (and paren, brace): remove it, or escape it.
pub(super) fn stray_closers(ctx: &Ctx<'_>, placed: &[Placed<'_>]) -> Vec<Draft> {
    let text = ctx.text;
    let mut out = Vec::new();
    for p in placed {
        if !matches!(
            p.diagnostic.message.as_str(),
            "unexpected closing bracket" | "unexpected closing paren" | "unexpected closing brace"
        ) {
            continue;
        }
        let Some(c) = text[p.from..]
            .chars()
            .next()
            .filter(|c| matches!(c, ']' | ')' | '}'))
        else {
            continue;
        };
        let end = p.from + c.len_utf8();
        let line = ctx.lines.line_of(p.from);
        out.push(Draft {
            id: format!("typst.remove-closer:{}", p.from),
            label: format!("Remove the extra {c} on line {line}"),
            why: format!("`{c}` closes nothing."),
            edits: vec![ByteEdit::replace(p.from, end, "")],
            confidence: Confidence::Likely,
            rule: RULE_STRAY,
            diagnostics: vec![p.index],
            finding: None,
        });
        out.push(Draft {
            id: format!("typst.escape-closer:{}", p.from),
            label: format!("Escape it as \\{c}"),
            why: format!("If `{c}` is meant as text, a backslash makes it so."),
            edits: vec![ByteEdit::insert(p.from, "\\")],
            confidence: Confidence::Likely,
            rule: RULE_STRAY,
            diagnostics: vec![p.index],
            finding: None,
        });
    }
    out
}

fn closer_name(open: char) -> &'static str {
    match open {
        '(' => ")",
        '[' => "]",
        '{' => "}",
        _ => "$",
    }
}

/// The text that closes `open` at `at`: math closes with a spaced `$` when it
/// opened with one.
fn closer_text(text: &str, at: usize, open: char) -> String {
    if open == '$' {
        let spaced = text[at + 1..].starts_with(char::is_whitespace);
        return if spaced {
            " $".to_owned()
        } else {
            "$".to_owned()
        };
    }
    closer_name(open).to_owned()
}

/// Byte range of the line holding `at`, without its line ending.
fn line_bounds(text: &str, at: usize) -> (usize, usize) {
    let start = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let end = text[at..].find(['\n', '\r']).map_or(text.len(), |p| at + p);
    (start, end)
}

/// End of the code on a line: before trailing spaces and a `//` comment.
fn trim_comment_end(text: &str, line_end: usize) -> usize {
    let start = text[..line_end].rfind('\n').map_or(0, |p| p + 1);
    let line = &text[start..line_end];
    let mut in_string = false;
    let mut escaped = false;
    let mut cut = line.len();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            in_string = !in_string;
        } else if !in_string && b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            cut = i;
            break;
        }
        i += 1;
    }
    start + line[..cut].trim_end().len()
}

/// Where the closer of `open` at `at` goes.
///
/// `(` and `{` (and a `[` that ends its line) hold an indented block: the
/// closer follows the last line indented deeper than the opener's line.
/// A `[` in the middle of a line and a `$` run to the end of the paragraph.
fn close_position(text: &str, at: usize, open: char) -> usize {
    let (line_start, line_end) = line_bounds(text, at);
    let opener_line = &text[line_start..line_end];
    let indent = opener_line.len() - opener_line.trim_start().len();
    let ends_line = opener_line.trim_end().ends_with(open);
    let by_indent = matches!(open, '(' | '{') || (open == '[' && ends_line);
    let mut last_end = line_end;
    let mut cursor = line_end;
    while cursor < text.len() {
        // `cursor` is at a line ending; step over it.
        let next_start = text[cursor..]
            .find('\n')
            .map_or(text.len(), |p| cursor + p + 1);
        if next_start >= text.len() {
            break;
        }
        let (s, e) = line_bounds(text, next_start);
        let line = &text[s..e];
        if line.trim().is_empty() {
            break;
        }
        let deeper = line.len() - line.trim_start().len() > indent;
        // A closing line of the block (`)` or `}` alone) belongs to it.
        let closes = by_indent && matches!(line.trim_start().chars().next(), Some(')' | '}' | ']'));
        if by_indent && !(deeper || closes) {
            break;
        }
        last_end = e;
        cursor = e;
    }
    trim_comment_end(text, last_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closer_goes_after_the_indented_body() {
        let t = "#figure(\n  image(\"a.png\"),\n  caption: [x],\n\nafter\n";
        assert_eq!(
            &t[..close_position(t, 7, '(')],
            "#figure(\n  image(\"a.png\"),\n  caption: [x],"
        );
        let t = "#set text(size: 11pt\nhello";
        assert_eq!(&t[..close_position(t, 9, '(')], "#set text(size: 11pt");
        let t = "#let a = [x // note\nmore\n\nnext";
        assert_eq!(&t[..close_position(t, 9, '[')], "#let a = [x // note\nmore");
        let t = "#f(1, 2 // trailing";
        assert_eq!(&t[..close_position(t, 2, '(')], "#f(1, 2");
    }

    #[test]
    fn comment_end_ignores_slashes_in_strings() {
        let t = "#link(\"http://x\" // c";
        assert_eq!(&t[..trim_comment_end(t, t.len())], "#link(\"http://x\"");
    }
}
