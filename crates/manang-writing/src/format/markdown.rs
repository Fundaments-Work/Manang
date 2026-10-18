//! A conservative Markdown normaliser.
//!
//! It never reflows prose and never touches code, math, front matter or
//! indented code. It only:
//!
//! * trims trailing whitespace (a hard line break, two or more trailing
//!   spaces, becomes exactly two);
//! * collapses runs of blank lines to at most `max_blank_lines`;
//! * removes blank lines at the start and end of the document and ends a
//!   non-empty document with a single newline.
//!
//! The transformation is line-based, so formatting a range is the same plan
//! applied to the lines the range touches.

use serde_json::Value;

use crate::error::WritingError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownOptions {
    pub max_blank_lines: usize,
    pub trim_trailing_whitespace: bool,
    pub final_newline: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            max_blank_lines: 1,
            trim_trailing_whitespace: true,
            final_newline: true,
        }
    }
}

impl MarkdownOptions {
    pub fn from_json(value: Option<&Value>) -> Result<Self, WritingError> {
        let mut out = Self::default();
        let Some(Value::Object(map)) = value.filter(|v| !v.is_null()) else {
            return match value {
                None | Some(Value::Null) => Ok(out),
                Some(_) => Err(WritingError::bad_arg("options", "expected an object")),
            };
        };
        for (key, v) in map {
            let bad =
                |what: &str| WritingError::bad_arg("options", format!("`{key}` must be {what}"));
            match key.as_str() {
                "max_blank_lines" => {
                    out.max_blank_lines = v
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|&n| n <= 20)
                        .ok_or_else(|| bad("an integer from 0 to 20"))?;
                }
                "trim_trailing_whitespace" => {
                    out.trim_trailing_whitespace = v.as_bool().ok_or_else(|| bad("a boolean"))?;
                }
                "final_newline" => {
                    out.final_newline = v.as_bool().ok_or_else(|| bad("a boolean"))?
                }
                other => {
                    return Err(WritingError::bad_arg(
                        "options",
                        format!("unknown markdown option `{other}`"),
                    ));
                }
            }
        }
        Ok(out)
    }
}

struct Line<'a> {
    text: &'a str,
    /// The line's original terminator: `""`, `"\n"` or `"\r\n"`.
    term: &'a str,
}

/// Dominant line ending: CRLF only when most terminators are CRLF.
fn eol_of(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    if crlf > 0 && crlf * 2 > text.matches('\n').count() {
        "\r\n"
    } else {
        "\n"
    }
}

fn split_lines(text: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(p) => {
                let (body, term) = match rest[..p].strip_suffix('\r') {
                    Some(body) => (body, &rest[body.len()..p + 1]),
                    None => (&rest[..p], &rest[p..p + 1]),
                };
                lines.push(Line { text: body, term });
                rest = &rest[p + 1..];
            }
            None => {
                lines.push(Line {
                    text: rest,
                    term: "",
                });
                break;
            }
        }
    }
    lines
}

fn blank(line: &str) -> bool {
    line.trim_matches([' ', '\t', '\r']).is_empty()
}

fn indented_code(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("    ")
}

/// Fence opener: (character, length).
fn fence_open(line: &str) -> Option<(char, usize)> {
    let t = line.trim_start_matches(' ');
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.chars().take_while(|&x| x == c).count();
    let info = &t[n..];
    (n >= 3 && !(c == '`' && info.contains('`'))).then_some((c, n))
}

fn fence_closes(line: &str, fence: (char, usize)) -> bool {
    let t = line.trim_matches([' ', '\t']);
    t.chars().count() >= fence.1 && t.chars().all(|c| c == fence.0)
}

/// Per-line verdict of what to do.
#[derive(Clone)]
enum Plan {
    Verbatim,
    Text(String),
    Drop,
}

fn plan(lines: &[Line<'_>], opts: &MarkdownOptions) -> Vec<Plan> {
    let mut plans = Vec::with_capacity(lines.len());
    // Front matter: only when it is closed.
    let front_end = (lines.first().is_some_and(|l| l.text.trim_end() == "---"))
        .then(|| {
            lines[1..]
                .iter()
                .position(|l| matches!(l.text.trim_end(), "---" | "..."))
                .map(|p| p + 1)
        })
        .flatten();
    // Leading blank lines are dropped, except before a `---`: without them it
    // would open front matter on the next pass and change the document.
    let keep_leading = front_end.is_none()
        && lines
            .iter()
            .find(|l| !blank(l.text))
            .is_some_and(|l| l.text.trim_end() == "---");
    let mut fence: Option<(char, usize)> = None;
    let mut math = false;
    let mut blank_run = 0usize;
    let mut emitted = false;
    for (i, line) in lines.iter().enumerate() {
        let text = line.text;
        if front_end.is_some_and(|end| i <= end) {
            plans.push(Plan::Verbatim);
            emitted = true;
            continue;
        }
        if let Some(open) = fence {
            if fence_closes(text, open) {
                fence = None;
            }
            plans.push(Plan::Verbatim);
            emitted = true;
            blank_run = 0;
            continue;
        }
        if math {
            if text.matches("$$").count() % 2 == 1 {
                math = false;
            }
            plans.push(Plan::Verbatim);
            emitted = true;
            blank_run = 0;
            continue;
        }
        if let Some(open) = fence_open(text) {
            fence = Some(open);
            plans.push(Plan::Verbatim);
            emitted = true;
            blank_run = 0;
            continue;
        }
        if text.matches("$$").count() % 2 == 1 {
            math = true;
            plans.push(Plan::Verbatim);
            emitted = true;
            blank_run = 0;
            continue;
        }
        if blank(text) {
            blank_run += 1;
            let next_indented = lines[i + 1..]
                .iter()
                .find(|l| !blank(l.text))
                .is_some_and(|l| indented_code(l.text));
            let over = blank_run > opts.max_blank_lines || (!emitted && !keep_leading);
            if over && !next_indented {
                plans.push(Plan::Drop);
            } else if opts.trim_trailing_whitespace {
                plans.push(Plan::Text(String::new()));
            } else {
                plans.push(Plan::Verbatim);
            }
            continue;
        }
        blank_run = 0;
        emitted = true;
        if indented_code(text) || !opts.trim_trailing_whitespace {
            plans.push(Plan::Verbatim);
            continue;
        }
        let trimmed = text.trim_end_matches([' ', '\t']);
        let trailing_spaces = text[trimmed.len()..].chars().filter(|&c| c == ' ').count();
        let heading = {
            let t = text.trim_start_matches(' ');
            let hashes = t.chars().take_while(|&c| c == '#').count();
            (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') && text.len() - t.len() < 4
        };
        let hard_break =
            !heading && text[trimmed.len()..].chars().all(|c| c == ' ') && trailing_spaces >= 2;
        let new = if hard_break {
            format!("{trimmed}  ")
        } else {
            trimmed.to_owned()
        };
        plans.push(if new == text {
            Plan::Verbatim
        } else {
            Plan::Text(new)
        });
    }
    plans
}

/// Normalise a whole document.
pub fn format_document(source: &str, opts: &MarkdownOptions) -> String {
    let eol = eol_of(source);
    let lines = split_lines(source);
    let plans = plan(&lines, opts);
    let mut out: Vec<(&str, bool)> = Vec::with_capacity(lines.len());
    for (line, p) in lines.iter().zip(&plans) {
        match p {
            Plan::Verbatim => out.push((line.text, false)),
            Plan::Text(t) => out.push((t.as_str(), blank(t))),
            Plan::Drop => {}
        }
    }
    let source_has_eol = lines.last().is_some_and(|l| !l.term.is_empty());
    // Trailing blank lines are never meaningful outside verbatim blocks.
    while out.last().is_some_and(|&(_, trimmable)| trimmable) {
        out.pop();
    }
    let mut result = String::with_capacity(source.len());
    for (i, (text, _)) in out.iter().enumerate() {
        result.push_str(text);
        if i + 1 < out.len() || opts.final_newline || source_has_eol {
            result.push_str(eol);
        }
    }
    result
}

/// Normalise only the lines that overlap the byte range `from..to`. Returns
/// the byte range of the replaced lines and their replacement text.
pub fn format_range(
    source: &str,
    from: usize,
    to: usize,
    opts: &MarkdownOptions,
) -> (std::ops::Range<usize>, String) {
    let lines = split_lines(source);
    let plans = plan(&lines, opts);
    // Byte start of each line.
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut at = 0;
    for line in &lines {
        starts.push(at);
        at += line.text.len() + line.term.len();
    }
    starts.push(at);
    let first = starts.partition_point(|&s| s <= from).saturating_sub(1);
    let last = starts
        .partition_point(|&s| s < to.max(from + 1))
        .saturating_sub(1)
        .max(first);
    let last = last.min(lines.len().saturating_sub(1));
    if lines.is_empty() {
        return (0..0, String::new());
    }
    let mut replacement = String::new();
    for i in first..=last {
        match &plans[i] {
            Plan::Verbatim => replacement.push_str(lines[i].text),
            Plan::Text(t) => replacement.push_str(t),
            Plan::Drop => continue,
        }
        replacement.push_str(lines[i].term);
    }
    (starts[first]..starts[last + 1], replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> String {
        format_document(s, &MarkdownOptions::default())
    }

    #[test]
    fn trims_collapses_and_terminates() {
        assert_eq!(
            fmt("\n\n# T  \n\n\n\ntext \nmore\t\n\n\n"),
            "# T\n\ntext\nmore\n"
        );
        assert_eq!(fmt("a"), "a\n");
        assert_eq!(fmt(""), "");
        assert_eq!(fmt("\n \n"), "");
    }

    #[test]
    fn hard_breaks_are_preserved_as_two_spaces() {
        assert_eq!(fmt("one   \ntwo  \nthree \n"), "one  \ntwo  \nthree\n");
    }

    #[test]
    fn code_math_front_matter_and_indented_code_are_untouched() {
        let doc = "---\ntitle: x  \n\n\n---\n\n```rust\nlet a = 1;   \n\n\n\nlet b;\n```\n\n$$\nx  \n\n\n= y\n$$\n\n    indented   \n\n\n    more\n";
        assert_eq!(fmt(doc), doc);
    }

    #[test]
    fn a_leading_thematic_break_does_not_become_front_matter() {
        // Dropping the blank line would make `---` open front matter next time.
        let doc = "\n---\n```\n---\n\n";
        assert_eq!(fmt(doc), doc);
        assert_eq!(fmt(&fmt(doc)), fmt(doc));
        assert_eq!(fmt("\n\n\ntext\n"), "text\n");
    }

    #[test]
    fn math_is_not_reflowed_inline() {
        assert_eq!(fmt("text $a   +   b$ end  \n"), "text $a   +   b$ end  \n");
    }

    #[test]
    fn crlf_is_preserved() {
        assert_eq!(fmt("a  \r\n\r\n\r\nb \r\n"), "a  \r\n\r\nb\r\n");
    }

    #[test]
    fn mixed_line_endings_become_the_dominant_one() {
        assert_eq!(fmt("\n\r\n"), "");
        assert_eq!(fmt("a\r\nb\nc\r\n"), "a\r\nb\r\nc\r\n");
        assert_eq!(fmt("a\nb\r\nc\n"), "a\nb\nc\n");
    }

    #[test]
    fn unterminated_fence_is_verbatim_to_the_end() {
        let doc = "```\ncode  \n\n\n";
        assert_eq!(fmt(doc), doc);
    }

    #[test]
    fn options_toggle_behaviour() {
        let opts = MarkdownOptions {
            max_blank_lines: 2,
            trim_trailing_whitespace: false,
            final_newline: false,
        };
        assert_eq!(format_document("a  \n\n\n\nb", &opts), "a  \n\n\nb");
    }

    #[test]
    fn range_touches_only_selected_lines() {
        let src = "a  \n\n\n\nb  \nc  \n";
        // Select line 5 ("b  ") only.
        let from = src.find('b').unwrap_or(0);
        let (range, text) = format_range(src, from, from + 1, &MarkdownOptions::default());
        assert_eq!(&src[range.clone()], "b  \n");
        assert_eq!(text, "b  \n");
        // Select the blank run: the surplus blanks go.
        let (range, text) = format_range(src, 4, 7, &MarkdownOptions::default());
        assert_eq!(&src[range], "\n\n\n");
        assert_eq!(text, "\n");
    }
}
