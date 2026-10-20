//! "Versions": the non-developer face of commits. Save a version (stage all
//! and commit with a recognisable trailer), list versions, and show what a
//! version changed. Reads are pure `gix`; the one write goes through
//! [`writes::commit_request`].

use gix::object::tree::diff::ChangeDetached;
use serde::Serialize;

use crate::ctx::Ctx;
use crate::diff::{self, FileDiff, MAX_DIFF_BYTES, Sides};
use crate::error::GitError;
use crate::history;
use crate::runner::Runner;
use crate::writes::{self, CommitRequest};

/// Trailer that marks a commit made by "Save a version".
pub const VERSION_TRAILER: &str = "Manang-Version: 1";
const MAX_NAME_CHARS: usize = 200;
/// Most files listed for one version.
pub const MAX_VERSION_FILES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Saved {
    pub sha: String,
    pub short: String,
    pub subject: String,
    pub files_changed: usize,
}

/// `(year, month, day)` of a day count since 1970-01-01 (proleptic
/// Gregorian; Howard Hinnant's civil-from-days).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `Version of 2026-10-18 14:32 UTC` for a Unix time in seconds.
pub fn default_name(unix_secs: i64) -> String {
    let (y, m, d) = civil_from_days(unix_secs.div_euclid(86_400));
    let rem = unix_secs.rem_euclid(86_400);
    format!(
        "Version of {y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

/// Number of files staged in the project (relative to the project root).
fn staged_count(ctx: &Ctx) -> Result<usize, GitError> {
    let out = Runner::new(ctx.root.path()).run_ok(&[
        "diff",
        "--cached",
        "--name-only",
        "--relative",
        "-z",
    ])?;
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .count())
}

/// Stage every non-ignored change and commit it as a labelled version.
/// An empty change set is [`GitError::NothingToSave`], never an empty commit.
///
/// `trailers` are extra `Key: value` lines (such as `Co-authored-by:` for
/// the people who edited together live) added after the version marker.
pub fn save_version(
    ctx: &Ctx,
    name: Option<&str>,
    message: Option<&str>,
    trailers: &[String],
) -> Result<Saved, GitError> {
    save_version_with(ctx, name, message, trailers, now_secs())
}

pub(crate) fn save_version_at(
    ctx: &Ctx,
    name: Option<&str>,
    message: Option<&str>,
    now: i64,
) -> Result<Saved, GitError> {
    save_version_with(ctx, name, message, &[], now)
}

fn save_version_with(
    ctx: &Ctx,
    name: Option<&str>,
    message: Option<&str>,
    extra_trailers: &[String],
    now: i64,
) -> Result<Saved, GitError> {
    let name = name.map(str::trim).filter(|n| !n.is_empty());
    if let Some(n) = name
        && (n.chars().count() > MAX_NAME_CHARS || n.chars().any(char::is_control))
    {
        return Err(GitError::BadArgs(
            "a version name is one line of at most 200 characters".to_owned(),
        ));
    }
    let subject = name.map_or_else(|| default_name(now), str::to_owned);
    let body = message.map(str::trim).filter(|m| !m.is_empty());
    let text = match body {
        Some(b) => format!("{subject}\n\n{b}"),
        None => subject,
    };
    writes::require_identity(ctx)?;
    writes::stage(ctx, &[".".to_owned()])?;
    let files_changed = staged_count(ctx)?;
    if files_changed == 0 {
        return Err(GitError::NothingToSave);
    }
    let mut trailers = vec![VERSION_TRAILER.to_owned()];
    trailers.extend(extra_trailers.iter().cloned());
    let committed = writes::commit_request(
        ctx,
        &CommitRequest {
            message: &text,
            trailers: &trailers,
            ..CommitRequest::default()
        },
    )?;
    Ok(Saved {
        sha: committed.hash,
        short: committed.short,
        subject: committed.subject,
        files_changed,
    })
}

/// Resolve user input to a commit id. Only a conservative character set is
/// accepted, so nothing resembling an option can reach git.
pub(crate) fn resolve_commit(ctx: &Ctx, rev: &str) -> Result<gix::ObjectId, GitError> {
    let ok_shape = !rev.is_empty()
        && rev.len() <= 256
        && !rev.starts_with('-')
        && !rev.contains("..")
        && rev.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '/' | '-' | '~' | '^' | '@' | '{' | '}')
        });
    if !ok_shape {
        return Err(GitError::BadArgs(format!(
            "invalid revision `{}`",
            rev.chars().take(60).collect::<String>()
        )));
    }
    let id = ctx
        .repo
        .rev_parse_single(rev)
        .map_err(|_| GitError::RevNotFound(rev.to_owned()))?;
    let commit = id
        .object()
        .map_err(GitError::backend)?
        .peel_to_commit()
        .map_err(|_| GitError::RevNotFound(rev.to_owned()))?;
    Ok(commit.id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionInfo {
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub author_name: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    pub files_changed: usize,
    /// Made by "Save a version" (carries the `Manang-Version` trailer).
    pub labelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionsPage {
    pub versions: Vec<VersionInfo>,
    /// Pass back as `cursor` for the next page.
    pub next_cursor: Option<String>,
}

/// One changed file of a commit, project-relative.
struct Change {
    path: String,
    old: Option<gix::ObjectId>,
    new: Option<gix::ObjectId>,
}

fn tree_of(ctx: &Ctx, id: gix::ObjectId) -> Result<gix::Tree<'_>, GitError> {
    ctx.repo
        .find_commit(id)
        .map_err(GitError::backend)?
        .tree()
        .map_err(GitError::backend)
}

/// Files of `commit` that differ from its first parent (the empty tree for
/// a root commit), limited to the project.
fn changes(ctx: &Ctx, commit: gix::ObjectId) -> Result<Vec<Change>, GitError> {
    let found = ctx.repo.find_commit(commit).map_err(GitError::backend)?;
    let new_tree = tree_of(ctx, commit)?;
    let old_tree = match found.parent_ids().next() {
        Some(parent) => tree_of(ctx, parent.detach())?,
        None => ctx.repo.empty_tree(),
    };
    let raw = ctx
        .repo
        .diff_tree_to_tree(&old_tree, &new_tree, gix::diff::Options::default())
        .map_err(GitError::backend)?;
    let mut out = Vec::new();
    for change in raw {
        let (location, old, new, mode) = match change {
            ChangeDetached::Addition {
                location,
                id,
                entry_mode,
                ..
            } => (location, None, Some(id), entry_mode),
            ChangeDetached::Deletion {
                location,
                id,
                entry_mode,
                ..
            } => (location, Some(id), None, entry_mode),
            ChangeDetached::Modification {
                location,
                previous_id,
                id,
                entry_mode,
                ..
            } => (location, Some(previous_id), Some(id), entry_mode),
            ChangeDetached::Rewrite {
                location,
                source_id,
                id,
                entry_mode,
                ..
            } => (location, Some(source_id), Some(id), entry_mode),
        };
        if mode.is_tree() || mode.is_commit() {
            continue;
        }
        let full = gix::bstr::ByteSlice::to_str_lossy(location.as_slice()).into_owned();
        if let Some(rel) = full.strip_prefix(ctx.prefix.as_str()) {
            out.push(Change {
                path: rel.to_owned(),
                old,
                new,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn is_labelled(commit: &gix::Commit<'_>) -> bool {
    commit
        .message_raw_sloppy()
        .to_string()
        .lines()
        .any(|l| l.trim_end() == VERSION_TRAILER)
}

/// The version timeline, newest first. `path` limits it to versions that
/// changed that file or folder; `cursor` is the previous page's `next_cursor`.
pub fn versions(
    ctx: &Ctx,
    rel: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<VersionsPage, GitError> {
    let offset = match cursor {
        None => 0,
        Some(c) => c
            .parse::<usize>()
            .map_err(|_| GitError::BadArgs("invalid cursor".to_owned()))?,
    };
    let page = history::log(ctx, rel, limit, offset)?;
    let mut versions = Vec::with_capacity(page.commits.len());
    for info in &page.commits {
        let id = gix::ObjectId::from_hex(info.hash.as_bytes()).map_err(GitError::backend)?;
        let commit = ctx.repo.find_commit(id).map_err(GitError::backend)?;
        versions.push(VersionInfo {
            hash: info.hash.clone(),
            short: info.short.clone(),
            subject: info.subject.clone(),
            author_name: info.author_name.clone(),
            time: info.time,
            files_changed: changes(ctx, id)?.len(),
            labelled: is_labelled(&commit),
        });
    }
    let next_cursor = page.has_more.then(|| (offset + versions.len()).to_string());
    Ok(VersionsPage {
        versions,
        next_cursor,
    })
}

fn blob(ctx: &Ctx, id: Option<gix::ObjectId>) -> Result<(Option<Vec<u8>>, bool), GitError> {
    let Some(id) = id else {
        return Ok((None, false));
    };
    let obj = ctx.repo.find_object(id).map_err(GitError::backend)?;
    if obj.data.len() as u64 > MAX_DIFF_BYTES {
        return Ok((None, true));
    }
    Ok((Some(obj.data.clone()), false))
}

fn sides(ctx: &Ctx, change: &Change) -> Result<Sides, GitError> {
    let (old, old_big) = blob(ctx, change.old)?;
    let (new, new_big) = blob(ctx, change.new)?;
    Ok(Sides {
        path: change.path.clone(),
        old,
        new,
        too_large: old_big || new_big,
    })
}

/// What a version changed: every file with its status and line counts, no
/// hunks (ask [`version_diff`] for one file's hunks).
pub fn version_files(ctx: &Ctx, rev: &str) -> Result<Vec<FileDiff>, GitError> {
    let id = resolve_commit(ctx, rev)?;
    let all = changes(ctx, id)?;
    let mut files = Vec::new();
    for change in all.iter().take(MAX_VERSION_FILES) {
        let mut file = diff::diff_sides(&sides(ctx, change)?);
        file.hunks.clear();
        files.push(file);
    }
    Ok(files)
}

/// One file's structured diff for a version (against its first parent).
pub fn version_diff(ctx: &Ctx, rev: &str, rel: &str) -> Result<FileDiff, GitError> {
    let id = resolve_commit(ctx, rev)?;
    let change = changes(ctx, id)?
        .into_iter()
        .find(|c| c.path == rel)
        .ok_or_else(|| GitError::PathNotInRev(rel.to_owned()))?;
    Ok(diff::diff_sides(&sides(ctx, &change)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_use_utc_dates() {
        assert_eq!(default_name(0), "Version of 1970-01-01 00:00 UTC");
        // 2026-10-18 14:32:09 UTC
        assert_eq!(
            default_name(1_792_333_929),
            "Version of 2026-10-18 14:32 UTC"
        );
        // 2024-02-29 (leap day) 23:59
        assert_eq!(
            default_name(1_709_251_199),
            "Version of 2024-02-29 23:59 UTC"
        );
    }
}
