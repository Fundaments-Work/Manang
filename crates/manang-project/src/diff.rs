//! A line diff of a project file against text the caller holds (unsaved edits,
//! a recovery snapshot), in the same shape as `git.diff` hunks so the shell
//! reuses one diff component.

use serde::Serialize;
use similar::{ChangeTag, TextDiff};

/// Inputs over this many bytes (together) are not diffed.
pub const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;
const CONTEXT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub kind: &'static str,
    pub text: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub old_offset: Option<u32>,
    pub new_offset: Option<u32>,
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    pub path: String,
    /// `added` when the old side is missing, else `modified`.
    pub status: &'static str,
    pub binary: bool,
    pub too_large: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

fn line_no(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Diff `old` (the file on disk, `None` if it does not exist) against `new`.
pub fn diff_text(path: &str, old: Option<&str>, new: &str) -> FileDiff {
    let status = if old.is_none() { "added" } else { "modified" };
    let old = old.unwrap_or("");
    let mut out = FileDiff {
        path: path.to_owned(),
        status,
        binary: false,
        too_large: false,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
    };
    if old.len() + new.len() > MAX_DIFF_BYTES {
        out.too_large = true;
        return out;
    }
    let diff = TextDiff::from_lines(old, new);
    for group in diff.grouped_ops(CONTEXT) {
        let mut lines = Vec::new();
        let (mut old_start, mut new_start) = (0u32, 0u32);
        let (mut old_lines, mut new_lines) = (0u32, 0u32);
        for op in &group {
            for change in diff.iter_changes(op) {
                let old_line = change.old_index().map(|i| line_no(i + 1));
                let new_line = change.new_index().map(|i| line_no(i + 1));
                if old_start == 0 {
                    old_start = old_line.unwrap_or(0);
                }
                if new_start == 0 {
                    new_start = new_line.unwrap_or(0);
                }
                let kind = match change.tag() {
                    ChangeTag::Equal => {
                        old_lines += 1;
                        new_lines += 1;
                        "context"
                    }
                    ChangeTag::Delete => {
                        old_lines += 1;
                        out.deletions += 1;
                        "remove"
                    }
                    ChangeTag::Insert => {
                        new_lines += 1;
                        out.additions += 1;
                        "add"
                    }
                };
                let value = change.value();
                lines.push(DiffLine {
                    kind,
                    text: value.trim_end_matches(['\n', '\r']).to_owned(),
                    old_line,
                    new_line,
                    old_offset: None,
                    new_offset: None,
                    no_newline: !value.ends_with('\n'),
                });
            }
        }
        out.hunks.push(DiffHunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            header: format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
            lines,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_line_shows_as_a_remove_and_an_add_with_context() {
        let d = diff_text("a.typ", Some("one\ntwo\nthree\n"), "one\nTWO\nthree\n");
        assert_eq!((d.additions, d.deletions), (1, 1));
        assert_eq!(d.hunks.len(), 1);
        let kinds: Vec<_> = d.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, ["context", "remove", "add", "context"]);
        assert_eq!(d.hunks[0].header, "@@ -1,3 +1,3 @@");
    }

    #[test]
    fn identical_text_has_no_hunks_and_a_missing_file_is_all_additions() {
        assert!(diff_text("a", Some("same\n"), "same\n").hunks.is_empty());
        let d = diff_text("a", None, "x\ny\n");
        assert_eq!(d.status, "added");
        assert_eq!(d.additions, 2);
    }

    #[test]
    fn huge_inputs_are_not_diffed() {
        let big = "x\n".repeat(MAX_DIFF_BYTES);
        let d = diff_text("a", Some(&big), "y");
        assert!(d.too_large && d.hunks.is_empty());
    }
}
