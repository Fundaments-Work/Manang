//! Structured and unified diffs.
//!
//! Content comes from `gix` (HEAD tree, index, work tree); the line diff is
//! computed by `similar` (pure Rust, Myers). Offsets are **UTF-16 code unit**
//! offsets from the start of the file, i.e. JavaScript string indices and
//! CodeMirror document positions, so the UI can place gutter marks without
//! re-counting.

use std::fs;

use manang_project::git::ChangeState;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

use crate::ctx::Ctx;
use crate::error::GitError;
use crate::status;

/// Largest single side (bytes) that is diffed; bigger files are flagged.
pub const MAX_DIFF_BYTES: u64 = 8 * 1024 * 1024;
const CONTEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineKind {
    Context,
    Add,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Line text without its terminator (`\n` or `\r\n`).
    pub text: String,
    /// 1-based line number in the old file (`None` for additions).
    pub old_line: Option<usize>,
    /// 1-based line number in the new file (`None` for removals).
    pub new_line: Option<usize>,
    /// UTF-16 offset of the line start in the old file.
    pub old_offset: Option<usize>,
    /// UTF-16 offset of the line start in the new file.
    pub new_offset: Option<usize>,
    /// The line has no trailing newline in its file.
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    /// 1-based first line (for an empty range: the line before).
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
    pub binary: bool,
    pub too_large: bool,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<Hunk>,
}

/// The two sides of one file's comparison.
pub struct Sides {
    pub path: String,
    pub old: Option<Vec<u8>>,
    pub new: Option<Vec<u8>>,
    pub too_large: bool,
}

impl Sides {
    fn status(&self) -> FileStatus {
        match (&self.old, &self.new) {
            (None, Some(_)) => FileStatus::Added,
            (Some(_), None) => FileStatus::Deleted,
            _ => FileStatus::Modified,
        }
    }

    fn binary(&self) -> bool {
        [&self.old, &self.new]
            .into_iter()
            .flatten()
            .any(|b| is_binary(b))
    }
}

/// Git's heuristic: a NUL byte in the first 8000 bytes.
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// UTF-16 start offset of every line (terminators included), plus one entry
/// for the end of the text.
fn line_offsets(text: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        at += utf16_len(line);
        offsets.push(at);
    }
    offsets
}

fn strip_eol(line: &str) -> (&str, bool) {
    match line.strip_suffix('\n') {
        Some(rest) => (rest.strip_suffix('\r').unwrap_or(rest), false),
        None => (line, true),
    }
}

/// Compute the structured diff of two byte buffers.
pub fn diff_sides(sides: &Sides) -> FileDiff {
    let mut file = FileDiff {
        path: sides.path.clone(),
        status: sides.status(),
        binary: false,
        too_large: sides.too_large,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
    };
    if sides.too_large {
        return file;
    }
    if sides.binary() {
        file.binary = true;
        return file;
    }
    let old = String::from_utf8_lossy(sides.old.as_deref().unwrap_or_default());
    let new = String::from_utf8_lossy(sides.new.as_deref().unwrap_or_default());
    let old_off = line_offsets(&old);
    let new_off = line_offsets(&new);
    let diff = TextDiff::from_lines(old.as_ref(), new.as_ref());
    for group in diff.grouped_ops(CONTEXT) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_from = first.old_range().start;
        let old_to = last.old_range().end;
        let new_from = first.new_range().start;
        let new_to = last.new_range().end;
        let old_lines = old_to - old_from;
        let new_lines = new_to - new_from;
        let old_start = if old_lines == 0 {
            old_from
        } else {
            old_from + 1
        };
        let new_start = if new_lines == 0 {
            new_from
        } else {
            new_from + 1
        };
        let mut lines = Vec::new();
        for op in &group {
            for change in diff.iter_changes(op) {
                let (text, no_newline) = strip_eol(change.value());
                let oi = change.old_index();
                let ni = change.new_index();
                let kind = match change.tag() {
                    ChangeTag::Equal => LineKind::Context,
                    ChangeTag::Insert => {
                        file.additions += 1;
                        LineKind::Add
                    }
                    ChangeTag::Delete => {
                        file.deletions += 1;
                        LineKind::Remove
                    }
                };
                lines.push(DiffLine {
                    kind,
                    text: text.to_owned(),
                    old_line: oi.map(|i| i + 1),
                    new_line: ni.map(|i| i + 1),
                    old_offset: oi.and_then(|i| old_off.get(i).copied()),
                    new_offset: ni.and_then(|i| new_off.get(i).copied()),
                    no_newline,
                });
            }
        }
        file.hunks.push(Hunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            header: format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
            lines,
        });
    }
    file
}

/// Unified text for one file, in `git diff` style.
pub fn unified(sides: &Sides) -> String {
    let path = &sides.path;
    let mut out = format!("diff --git a/{path} b/{path}\n");
    match sides.status() {
        FileStatus::Added => out.push_str("new file\n"),
        FileStatus::Deleted => out.push_str("deleted file\n"),
        FileStatus::Modified => {}
    }
    if sides.too_large {
        out.push_str("File too large to diff\n");
        return out;
    }
    if sides.binary() {
        out.push_str(&format!("Binary files a/{path} and b/{path} differ\n"));
        return out;
    }
    let old = String::from_utf8_lossy(sides.old.as_deref().unwrap_or_default());
    let new = String::from_utf8_lossy(sides.new.as_deref().unwrap_or_default());
    let old_name = if sides.old.is_some() {
        format!("a/{path}")
    } else {
        "/dev/null".to_owned()
    };
    let new_name = if sides.new.is_some() {
        format!("b/{path}")
    } else {
        "/dev/null".to_owned()
    };
    let text = TextDiff::from_lines(old.as_ref(), new.as_ref());
    out.push_str(
        &text
            .unified_diff()
            .context_radius(CONTEXT)
            .header(&old_name, &new_name)
            .to_string(),
    );
    out
}

fn blob_bytes(
    repo: &gix::Repository,
    id: gix::ObjectId,
    what: &str,
) -> Result<(Vec<u8>, bool), GitError> {
    let obj = repo.find_object(id).map_err(GitError::backend)?;
    if obj.data.len() as u64 > MAX_DIFF_BYTES {
        return Err(GitError::TooLarge {
            what: what.to_owned(),
            size: obj.data.len() as u64,
            limit: MAX_DIFF_BYTES,
        });
    }
    Ok((obj.data.clone(), false))
}

/// The blob of `repo_path` in `HEAD`, `None` when absent or unborn.
pub fn head_blob(ctx: &Ctx, rel: &str) -> Result<Option<Vec<u8>>, GitError> {
    let Ok(commit) = ctx.repo.head_commit() else {
        return Ok(None);
    };
    let tree = commit.tree().map_err(GitError::backend)?;
    let repo_path = ctx.repo_path(rel);
    let entry = tree
        .lookup_entry_by_path(&repo_path)
        .map_err(GitError::backend)?;
    match entry {
        Some(e) if e.mode().is_blob() => {
            blob_bytes(&ctx.repo, e.object_id(), rel).map(|(b, _)| Some(b))
        }
        _ => Ok(None),
    }
}

/// The staged blob of `rel` (stage 0, falling back to "ours" in a conflict).
pub fn index_blob(
    ctx: &Ctx,
    index: &gix::worktree::Index,
    rel: &str,
) -> Result<Option<Vec<u8>>, GitError> {
    let repo_path = ctx.repo_path(rel);
    let key: &gix::bstr::BStr = repo_path.as_str().into();
    let entry = index
        .entry_by_path(key)
        .or_else(|| index.entry_by_path_and_stage(key, gix::index::entry::Stage::Ours));
    match entry {
        Some(e) => blob_bytes(&ctx.repo, e.id, rel).map(|(b, _)| Some(b)),
        None => Ok(None),
    }
}

/// The work-tree content of `rel`, `None` when the file is gone. A symlink
/// yields its target text, as git stores it.
pub fn worktree_bytes(ctx: &Ctx, rel: &str) -> Result<Option<(Vec<u8>, bool)>, GitError> {
    let full = ctx.root.resolve_entry(rel)?;
    let meta = match fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(GitError::io(format!("reading {rel}"), e)),
    };
    if meta.file_type().is_symlink() {
        let target = fs::read_link(&full).map_err(|e| GitError::io(format!("reading {rel}"), e))?;
        return Ok(Some((
            target.to_string_lossy().into_owned().into_bytes(),
            false,
        )));
    }
    if meta.is_dir() {
        return Ok(None);
    }
    if meta.len() > MAX_DIFF_BYTES {
        return Ok(Some((Vec::new(), true)));
    }
    fs::read(&full)
        .map(|b| Some((b, false)))
        .map_err(|e| GitError::io(format!("reading {rel}"), e))
}

fn in_scope(path: &str, scope: &str) -> bool {
    scope.is_empty()
        || path == scope
        || path.strip_prefix(scope).is_some_and(|r| r.starts_with('/'))
}

/// Collect the comparison sides for every changed file under `scope`
/// (project-relative file or directory; empty means everything).
pub fn collect(ctx: &Ctx, scope: &str, staged: bool) -> Result<Vec<Sides>, GitError> {
    let st = status::changes(ctx)?;
    let index = ctx.repo.index_or_empty().map_err(GitError::backend)?;
    let mut out = Vec::new();
    for change in st.changes.iter().filter(|c| in_scope(&c.path, scope)) {
        if staged {
            if change.staged.is_none() {
                continue;
            }
            let old = head_blob(ctx, &change.path)?;
            let new = index_blob(ctx, &index, &change.path)?;
            out.push(Sides {
                path: change.path.clone(),
                old,
                new,
                too_large: false,
            });
        } else {
            if change.unstaged.is_none() {
                continue;
            }
            let old = if change.unstaged == Some(ChangeState::Untracked) {
                None
            } else {
                index_blob(ctx, &index, &change.path)?
            };
            let (new, too_large) = match worktree_bytes(ctx, &change.path)? {
                Some((b, big)) => (Some(b), big),
                None => (None, false),
            };
            if old.is_none() && new.is_none() {
                continue;
            }
            out.push(Sides {
                path: change.path.clone(),
                old,
                new,
                too_large,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sides(old: Option<&str>, new: Option<&str>) -> Sides {
        Sides {
            path: "a.txt".to_owned(),
            old: old.map(|s| s.as_bytes().to_vec()),
            new: new.map(|s| s.as_bytes().to_vec()),
            too_large: false,
        }
    }

    #[test]
    fn modification_has_ranges_and_offsets() {
        let d = diff_sides(&sides(Some("a\nb\nc\n"), Some("a\nB\nc\nd\n")));
        assert_eq!(d.additions, 2);
        assert_eq!(d.deletions, 1);
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 3, 1, 4)
        );
        let added = h.lines.iter().find(|l| l.text == "B").map(|l| l.new_offset);
        assert_eq!(added, Some(Some(2)));
    }

    #[test]
    fn offsets_count_utf16_units() {
        let d = diff_sides(&sides(Some("😀\nx\n"), Some("😀\ny\n")));
        let l = d.hunks[0].lines.iter().find(|l| l.text == "y");
        assert_eq!(l.and_then(|l| l.new_offset), Some(3));
    }

    #[test]
    fn crlf_is_stripped_but_preserved_in_comparison() {
        let d = diff_sides(&sides(Some("a\r\nb\r\n"), Some("a\r\nc\r\n")));
        assert_eq!(d.hunks[0].lines.iter().filter(|l| l.text == "c").count(), 1);
        let same = diff_sides(&sides(Some("a\r\n"), Some("a\n")));
        assert_eq!((same.additions, same.deletions), (1, 1));
    }

    #[test]
    fn binary_and_missing_newline() {
        let mut s = sides(None, None);
        s.new = Some(vec![0, 1, 2]);
        let d = diff_sides(&s);
        assert!(d.binary && d.hunks.is_empty());
        assert_eq!(d.status, FileStatus::Added);
        let n = diff_sides(&sides(Some("a"), Some("a\nb")));
        assert!(n.hunks[0].lines.iter().any(|l| l.no_newline));
    }

    #[test]
    fn unified_text_has_git_headers() {
        let t = unified(&sides(Some("a\n"), Some("b\n")));
        assert!(t.starts_with("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@"));
        assert!(unified(&sides(None, Some("x\n"))).contains("--- /dev/null"));
    }
}
