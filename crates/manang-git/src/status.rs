//! `git.status`: the file list comes from `manang_project::git::status`
//! (gix), reused rather than re-implemented; this module adds upstream
//! tracking and flattens the per-file state for the UI.

use manang_project::git::{self as pgit, ChangeState, GitStatus};
use serde::Serialize;

use crate::ctx::Ctx;
use crate::error::GitError;

/// Raw gix status of the project (project-relative paths).
pub fn changes(ctx: &Ctx) -> Result<GitStatus, GitError> {
    pgit::status(ctx.root.path()).map_err(GitError::backend)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub path: String,
    /// HEAD vs index.
    pub staged: Option<ChangeState>,
    /// Index vs work tree (never `untracked`/`conflicted`; see the flags).
    pub unstaged: Option<ChangeState>,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Upstream {
    pub name: String,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReport {
    pub branch: Option<String>,
    pub detached: bool,
    /// The branch exists but has no commits yet.
    pub unborn: bool,
    pub head: Option<String>,
    pub upstream: Option<Upstream>,
    pub files: Vec<FileEntry>,
    pub truncated: bool,
}

/// Commits reachable from `from` but not from `hidden`.
fn count_exclusive(
    repo: &gix::Repository,
    from: gix::ObjectId,
    hidden: gix::ObjectId,
) -> Result<usize, GitError> {
    let walk = repo
        .rev_walk([from])
        .with_hidden([hidden])
        .all()
        .map_err(GitError::backend)?;
    Ok(walk.filter(Result::is_ok).count())
}

fn upstream(ctx: &Ctx, head: Option<gix::ObjectId>) -> Result<Option<Upstream>, GitError> {
    let repo = &ctx.repo;
    let Ok(head_ref) = repo.head() else {
        return Ok(None);
    };
    let (Some(name), Some(head_id)) = (head_ref.referent_name(), head) else {
        return Ok(None);
    };
    let Some(Ok(tracking)) =
        repo.branch_remote_tracking_ref_name(name, gix::remote::Direction::Fetch)
    else {
        return Ok(None);
    };
    let Ok(Some(mut reference)) = repo.try_find_reference(tracking.as_ref()) else {
        return Ok(None);
    };
    let Ok(up_id) = reference.peel_to_id() else {
        return Ok(None);
    };
    let up_id = up_id.detach();
    Ok(Some(Upstream {
        name: tracking.shorten().to_string(),
        ahead: count_exclusive(repo, head_id, up_id)?,
        behind: count_exclusive(repo, up_id, head_id)?,
    }))
}

pub fn status(ctx: &Ctx) -> Result<StatusReport, GitError> {
    let raw = changes(ctx)?;
    let head_id = ctx.repo.head_id().ok().map(gix::Id::detach);
    let unborn = ctx.repo.head().map(|h| h.is_unborn()).unwrap_or(false);
    let files = raw
        .changes
        .into_iter()
        .map(|c| {
            let conflicted = c.unstaged == Some(ChangeState::Conflicted);
            let untracked = c.unstaged == Some(ChangeState::Untracked);
            FileEntry {
                path: c.path,
                staged: c.staged,
                unstaged: if conflicted || untracked {
                    None
                } else {
                    c.unstaged
                },
                untracked,
                conflicted,
            }
        })
        .collect();
    Ok(StatusReport {
        branch: raw.branch,
        detached: raw.detached,
        unborn,
        head: raw.head,
        upstream: upstream(ctx, head_id)?,
        files,
        truncated: raw.truncated,
    })
}
