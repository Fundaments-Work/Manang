//! Markdown rules.
//!
//! Markdown has no compile errors: an unclosed fence just swallows the rest
//! of the document. These rules therefore scan the text themselves and report
//! [`Finding`](crate::Finding)s, each with its fix. They run whether or not
//! diagnostics were passed.

use crate::engine::{ByteEdit, Ctx, Draft, DraftFinding};
use crate::text::line_ending;
use crate::types::{Confidence, DiagnosticIn};

const RULE_FENCE: &str = "markdown.unclosed-fence";
const RULE_TABLE: &str = "markdown.table-delimiter";

pub(crate) fn suggest(ctx: &Ctx<'_>, _diagnostics: &[DiagnosticIn]) -> Vec<Draft> {
    let mut out = Vec::new();
    out.extend(unclosed_fence(ctx));
    out.extend(table_delimiters(ctx));
    out
}

/// A line of the document with its byte range (without line ending).
struct Line<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\n', '\r']);
        out.push(Line {
            text: body,
            start: offset,
            end: offset + body.len(),
        });
        offset += raw.len();
    }
    out
}

/// Index of the first line after a leading `---` front matter block.
fn after_front_matter(lines: &[Line<'_>]) -> usize {
    if lines.first().is_some_and(|l| l.text.trim_end() == "---")
        && let Some(close) = lines
            .iter()
            .skip(1)
            .position(|l| matches!(l.text.trim_end(), "---" | "..."))
    {
        return close + 2;
    }
    0
}

/// `(character, length)` of a fence line (``` or ~~~, at most 3 spaces of
/// indent), with whatever follows the fence.
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let c = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = rest.chars().take_while(|x| *x == c).count();
    let after = &rest[n..];
    // An info string of a backtick fence cannot contain backticks.
    (n >= 3 && !(c == '`' && after.contains('`'))).then_some((c, n, after))
}

fn unclosed_fence(ctx: &Ctx<'_>) -> Option<Draft> {
    let ls = lines(ctx.text);
    let mut open: Option<(usize, char, usize)> = None;
    for (i, line) in ls.iter().enumerate().skip(after_front_matter(&ls)) {
        match (open, fence_of(line.text)) {
            (None, Some((c, n, _))) => open = Some((i, c, n)),
            (Some((_, oc, on)), Some((c, n, after)))
                if c == oc && n >= on && after.trim().is_empty() =>
            {
                open = None;
            }
            _ => {}
        }
    }
    let (at, c, n) = open?;
    let eol = line_ending(ctx.text);
    let fence: String = std::iter::repeat_n(c, n).collect();
    let lead = if ctx.text.is_empty() || ctx.text.ends_with('\n') {
        ""
    } else {
        eol
    };
    let opener = &ls[at];
    Some(Draft {
        id: format!("markdown.close-fence:{}", opener.start),
        label: format!("Close the code fence with {fence}"),
        why: "The code fence opened here is never closed, so everything after it is code."
            .to_owned(),
        edits: vec![ByteEdit::insert(
            ctx.text.len(),
            format!("{lead}{fence}{eol}"),
        )],
        confidence: Confidence::Certain,
        rule: RULE_FENCE,
        diagnostics: Vec::new(),
        finding: Some(DraftFinding {
            message: "Code fence is never closed".to_owned(),
            severity: "warning",
            from: opener.start,
            to: opener.end,
            rule: RULE_FENCE,
        }),
    })
}

/// Cells of a table row: the text between unescaped pipes, ignoring one
/// leading and one trailing pipe.
fn cells(row: &str) -> Vec<&str> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = row.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'|' => {
                out.push(&row[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = &row[start.min(row.len())..];
    if !last.trim().is_empty() {
        out.push(last);
    }
    out
}

/// A delimiter row: every cell is `---`, `:--`, `--:` or `:-:`.
fn is_delimiter_row(line: &str) -> bool {
    if !line.contains('|') && !line.contains("--") {
        return false;
    }
    let cs = cells(line);
    !cs.is_empty()
        && cs.iter().all(|c| {
            let c = c.trim();
            let core = c.trim_start_matches(':').trim_end_matches(':');
            !core.is_empty() && core.chars().all(|x| x == '-')
        })
        && line.contains('|')
}

fn table_delimiters(ctx: &Ctx<'_>) -> Vec<Draft> {
    let ls = lines(ctx.text);
    let mut out = Vec::new();
    let mut in_fence: Option<(char, usize)> = None;
    let start = after_front_matter(&ls);
    for i in start..ls.len() {
        let line = &ls[i];
        match (in_fence, fence_of(line.text)) {
            (None, Some((c, n, _))) => {
                in_fence = Some((c, n));
                continue;
            }
            (Some((oc, on)), Some((c, n, after)))
                if c == oc && n >= on && after.trim().is_empty() =>
            {
                in_fence = None;
                continue;
            }
            (Some(_), _) => continue,
            _ => {}
        }
        if i == 0 || !is_delimiter_row(line.text) {
            continue;
        }
        let header = &ls[i - 1];
        if !header.text.contains('|') || header.text.trim().is_empty() {
            continue;
        }
        let (head, delim) = (cells(header.text).len(), cells(line.text).len());
        if head == delim || head == 0 || delim == 0 {
            continue;
        }
        // Rebuild the delimiter row with the header's width, keeping the
        // alignment markers it already has.
        let existing: Vec<String> = cells(line.text)
            .iter()
            .map(|c| c.trim().to_owned())
            .collect();
        let rebuilt: Vec<String> = (0..head)
            .map(|k| existing.get(k).cloned().unwrap_or_else(|| "---".to_owned()))
            .collect();
        let leading =
            line.text.trim_start().starts_with('|') || header.text.trim_start().starts_with('|');
        let trailing = line.text.trim_end().ends_with('|') || header.text.trim_end().ends_with('|');
        let indent = &line.text[..line.text.len() - line.text.trim_start().len()];
        let mut row = String::from(indent);
        if leading {
            row.push_str("| ");
        }
        row.push_str(&rebuilt.join(" | "));
        if trailing {
            row.push_str(" |");
        }
        let message = format!("Table delimiter row has {delim} columns but the header has {head}");
        out.push(Draft {
            id: format!("markdown.table-delimiter:{}", line.start),
            label: format!("Make the delimiter row {head} columns wide"),
            why: "Without a delimiter row as wide as the header, the table is not recognised."
                .to_owned(),
            edits: vec![ByteEdit::replace(line.start, line.end, row)],
            confidence: Confidence::Likely,
            rule: RULE_TABLE,
            diagnostics: Vec::new(),
            finding: Some(DraftFinding {
                message,
                severity: "warning",
                from: line.start,
                to: line.end,
                rule: RULE_TABLE,
            }),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cells() {
        assert_eq!(cells("| a | b |").len(), 2);
        assert_eq!(cells("a | b").len(), 2);
        assert_eq!(cells("| a \\| b | c |").len(), 2);
        assert!(is_delimiter_row("|---|:-:|"));
        assert!(is_delimiter_row("--- | ---"));
        assert!(!is_delimiter_row("---"));
        assert!(!is_delimiter_row("| a | b |"));
    }

    #[test]
    fn fences() {
        assert_eq!(fence_of("```rust"), Some(('`', 3, "rust")));
        assert_eq!(fence_of("  ~~~~"), Some(('~', 4, "")));
        assert_eq!(fence_of("    ```"), None);
        assert_eq!(fence_of("``` a`b"), None);
        assert_eq!(fence_of("``"), None);
    }
}
