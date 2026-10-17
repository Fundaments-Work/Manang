//! Read-only history queries, all pure-Rust `gix`: log, blame, branches, show.

use serde::Serialize;

use crate::ctx::Ctx;
use crate::error::GitError;

pub const MAX_LOG_LIMIT: usize = 1000;
pub const MAX_SHOW_BYTES: u64 = 32 * 1024 * 1024;
const SHORT: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitInfo {
    pub hash: String,
    pub short: String,
    pub author_name: String,
    pub author_email: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    pub subject: String,
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogPage {
    pub commits: Vec<CommitInfo>,
    /// More commits exist beyond `offset + limit`.
    pub has_more: bool,
}

fn short(hash: &str) -> String {
    hash.chars().take(SHORT).collect()
}

fn info(commit: &gix::Commit<'_>) -> Result<CommitInfo, GitError> {
    let hash = commit.id().to_string();
    let author = commit.author().map_err(GitError::backend)?;
    let time = author.time().map(|t| t.seconds).unwrap_or(0);
    let message = commit.message_raw_sloppy();
    let subject = message
        .to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    Ok(CommitInfo {
        short: short(&hash),
        hash,
        author_name: author.name.to_string(),
        author_email: author.email.to_string(),
        time,
        subject,
        parents: commit.parent_ids().map(|p| p.to_string()).collect(),
    })
}

fn entry_id(
    repo: &gix::Repository,
    commit: gix::ObjectId,
    path: &str,
) -> Result<Option<gix::ObjectId>, GitError> {
    let tree = repo
        .find_commit(commit)
        .map_err(GitError::backend)?
        .tree()
        .map_err(GitError::backend)?;
    Ok(tree
        .lookup_entry_by_path(path)
        .map_err(GitError::backend)?
        .map(|e| e.object_id()))
}

/// Commits reachable from HEAD, newest first, optionally only those that
/// changed `rel` (project-relative file or directory).
pub fn log(ctx: &Ctx, rel: &str, limit: usize, offset: usize) -> Result<LogPage, GitError> {
    let limit = limit.clamp(1, MAX_LOG_LIMIT);
    let empty = LogPage {
        commits: Vec::new(),
        has_more: false,
    };
    let Ok(head) = ctx.repo.head_id() else {
        return Ok(empty);
    };
    let repo_path = ctx.repo_path(rel);
    let repo_path = repo_path.trim_end_matches('/');
    let walk = ctx
        .repo
        .rev_walk([head.detach()])
        .sorting(gix::revision::walk::Sorting::ByCommitTime(
            gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
        ))
        .all()
        .map_err(GitError::backend)?;
    let mut seen = 0usize;
    let mut commits = Vec::new();
    let mut has_more = false;
    for item in walk {
        let item = item.map_err(GitError::backend)?;
        if !repo_path.is_empty() {
            let own = entry_id(&ctx.repo, item.id, repo_path)?;
            let parents: Vec<gix::ObjectId> = item.parent_ids().map(|p| p.detach()).collect();
            let mut touched = parents.is_empty() && own.is_some();
            for parent in &parents {
                if entry_id(&ctx.repo, *parent, repo_path)? != own {
                    touched = true;
                } else {
                    touched = false;
                    break;
                }
            }
            if !touched {
                continue;
            }
        }
        if seen < offset {
            seen += 1;
            continue;
        }
        if commits.len() == limit {
            has_more = true;
            break;
        }
        let commit = ctx.repo.find_commit(item.id).map_err(GitError::backend)?;
        commits.push(info(&commit)?);
    }
    Ok(LogPage { commits, has_more })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlameRange {
    /// 1-based first line in the file.
    pub start: usize,
    pub lines: usize,
    pub commit: CommitInfo,
    /// Path in that commit when the file was renamed since.
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blame {
    pub path: String,
    pub total_lines: usize,
    pub ranges: Vec<BlameRange>,
}

/// Blame `rel` as committed at HEAD (uncommitted edits are not attributed).
pub fn blame(ctx: &Ctx, rel: &str) -> Result<Blame, GitError> {
    if rel.is_empty() {
        return Err(GitError::BadArgs("`path` is required".to_owned()));
    }
    let head = ctx
        .repo
        .head_id()
        .map_err(|_| GitError::RevNotFound("HEAD".to_owned()))?;
    let repo_path = ctx.repo_path(rel);
    let outcome = ctx
        .repo
        .blame_file(
            repo_path.as_str().into(),
            head.detach(),
            gix::repository::blame_file::Options::default(),
        )
        .map_err(GitError::backend)?;
    let total_lines = outcome.blob.iter().filter(|b| **b == b'\n').count()
        + usize::from(outcome.blob.last().is_some_and(|b| *b != b'\n'));
    let mut cache: std::collections::HashMap<gix::ObjectId, CommitInfo> =
        std::collections::HashMap::new();
    let mut ranges = Vec::new();
    for e in &outcome.entries {
        let commit = match cache.get(&e.commit_id) {
            Some(c) => c.clone(),
            None => {
                let c = info(
                    &ctx.repo
                        .find_commit(e.commit_id)
                        .map_err(GitError::backend)?,
                )?;
                cache.insert(e.commit_id, c.clone());
                c
            }
        };
        ranges.push(BlameRange {
            start: e.start_in_blamed_file as usize + 1,
            lines: e.len.get() as usize,
            commit,
            source_path: e
                .source_file_name
                .as_ref()
                .map(|n| gix::bstr::ByteSlice::to_str_lossy(n.as_slice()).into_owned()),
        });
    }
    Ok(Blame {
        path: rel.to_owned(),
        total_lines,
        ranges,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub remote: bool,
    pub current: bool,
    pub hash: String,
    pub upstream: Option<String>,
}

pub fn branches(ctx: &Ctx) -> Result<Vec<BranchInfo>, GitError> {
    let repo = &ctx.repo;
    let current = repo
        .head()
        .ok()
        .and_then(|h| h.referent_name().map(|n| n.as_bstr().to_owned()));
    let platform = repo.references().map_err(GitError::backend)?;
    let mut out = Vec::new();
    for (remote, iter) in [
        (false, platform.local_branches().map_err(GitError::backend)?),
        (true, platform.remote_branches().map_err(GitError::backend)?),
    ] {
        for r in iter {
            let mut r = r.map_err(GitError::backend)?;
            let full = r.name().as_bstr().to_owned();
            let name = r.name().shorten().to_string();
            if remote && name.ends_with("/HEAD") {
                continue;
            }
            let Ok(id) = r.peel_to_id() else { continue };
            let upstream = if remote {
                None
            } else {
                repo.branch_remote_tracking_ref_name(r.name(), gix::remote::Direction::Fetch)
                    .and_then(Result::ok)
                    .map(|t| t.shorten().to_string())
            };
            out.push(BranchInfo {
                current: !remote && current.as_ref() == Some(&full),
                name,
                remote,
                hash: id.to_string(),
                upstream,
            });
        }
    }
    // An unborn current branch has no ref yet; still list it.
    if let Some(cur) = &current
        && !out.iter().any(|b| b.current)
        && repo.head().map(|h| h.is_unborn()).unwrap_or(false)
    {
        let name = gix::bstr::ByteSlice::to_str_lossy(cur.as_slice())
            .trim_start_matches("refs/heads/")
            .to_owned();
        out.push(BranchInfo {
            name,
            remote: false,
            current: true,
            hash: String::new(),
            upstream: None,
        });
    }
    out.sort_by(|a, b| (a.remote, &a.name).cmp(&(b.remote, &b.name)));
    Ok(out)
}

/// File content at a revision.
pub enum Shown {
    Text(String),
    Binary(Vec<u8>),
}

pub fn show(ctx: &Ctx, rev: &str, rel: &str) -> Result<Shown, GitError> {
    if rev.is_empty() || rev.starts_with('-') || rev.contains('\0') || rev.len() > 1024 {
        return Err(GitError::BadArgs(format!("invalid revision `{rev}`")));
    }
    if rel.is_empty() {
        return Err(GitError::BadArgs("`path` is required".to_owned()));
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
    let repo_path = ctx.repo_path(rel);
    let entry = commit
        .tree()
        .map_err(GitError::backend)?
        .lookup_entry_by_path(&repo_path)
        .map_err(GitError::backend)?
        .filter(|e| e.mode().is_blob())
        .ok_or_else(|| GitError::PathNotInRev(rel.to_owned()))?;
    let obj = entry.object().map_err(GitError::backend)?;
    if obj.data.len() as u64 > MAX_SHOW_BYTES {
        return Err(GitError::TooLarge {
            what: rel.to_owned(),
            size: obj.data.len() as u64,
            limit: MAX_SHOW_BYTES,
        });
    }
    let bytes = obj.data.clone();
    Ok(match String::from_utf8(bytes) {
        Ok(text) => Shown::Text(text),
        Err(e) => Shown::Binary(e.into_bytes()),
    })
}
