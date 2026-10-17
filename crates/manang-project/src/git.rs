//! Git-native status via `gix` (pure Rust, no libgit2 or `git` binary).
//!
//! The project may be the repository root or a sub-directory of one; only
//! changes inside the project are reported, with project-relative paths.
//! Anything that is not a usable work-tree repository (no repo, bare repo,
//! unreadable index) degrades to `is_repo: false` rather than an error, so
//! the UI can simply hide the git affordances.

use std::collections::BTreeMap;
use std::path::Path;

use gix::bstr::ByteSlice;
use gix::status::Item;
use gix::status::index_worktree::iter::Summary;
use serde::Serialize;

use crate::error::ProjectError;

/// Most changed files reported; `truncated` is set beyond this.
pub const MAX_CHANGES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeState {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Typechange,
    Conflicted,
    IntentToAdd,
    Untracked,
}

/// One changed file, in the style of `git status --porcelain` (`XY`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitChange {
    /// Project-relative, `/`-separated.
    pub path: String,
    /// `HEAD` vs index (the "X" column).
    pub staged: Option<ChangeState>,
    /// Index vs work tree (the "Y" column), including untracked files.
    pub unstaged: Option<ChangeState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitStatus {
    pub is_repo: bool,
    /// Short branch name, e.g. `main`. `None` when detached or not a repo.
    pub branch: Option<String>,
    pub detached: bool,
    /// Full commit id of `HEAD`, `None` on an unborn branch.
    pub head: Option<String>,
    pub changes: Vec<GitChange>,
    pub truncated: bool,
}

impl GitStatus {
    fn not_a_repo() -> Self {
        Self {
            is_repo: false,
            branch: None,
            detached: false,
            head: None,
            changes: Vec::new(),
            truncated: false,
        }
    }
}

fn from_summary(summary: Summary) -> ChangeState {
    match summary {
        Summary::Removed => ChangeState::Deleted,
        Summary::Added => ChangeState::Untracked,
        Summary::Modified => ChangeState::Modified,
        Summary::TypeChange => ChangeState::Typechange,
        Summary::Renamed => ChangeState::Renamed,
        Summary::Copied => ChangeState::Copied,
        Summary::IntentToAdd => ChangeState::IntentToAdd,
        Summary::Conflict => ChangeState::Conflicted,
    }
}

fn from_tree_index(change: &gix::diff::index::Change) -> ChangeState {
    use gix::diff::index::ChangeRef;
    match change {
        ChangeRef::Addition { .. } => ChangeState::Added,
        ChangeRef::Deletion { .. } => ChangeState::Deleted,
        ChangeRef::Modification { .. } => ChangeState::Modified,
        ChangeRef::Rewrite { copy, .. } => {
            if *copy {
                ChangeState::Copied
            } else {
                ChangeState::Renamed
            }
        }
    }
}

fn git_err(e: impl std::fmt::Display) -> ProjectError {
    ProjectError::Git(e.to_string())
}

/// Status of the repository containing `root` (a canonical project path).
pub fn status(root: &Path) -> Result<GitStatus, ProjectError> {
    let Ok(repo) = gix::discover(root) else {
        return Ok(GitStatus::not_a_repo());
    };
    let Some(workdir) = repo.workdir().and_then(|w| w.canonicalize().ok()) else {
        return Ok(GitStatus::not_a_repo()); // bare repository
    };
    let Ok(prefix) = root.strip_prefix(&workdir) else {
        return Ok(GitStatus::not_a_repo());
    };
    let prefix = prefix.to_path_buf();

    let head = repo.head().map_err(git_err)?;
    let branch = head
        .referent_name()
        .map(|name| name.shorten().to_str_lossy().into_owned());
    let detached = head.referent_name().is_none() && !head.is_unborn();
    let head_id = repo.head_id().ok().map(|id| id.to_string());

    let platform = repo
        .status(gix::progress::Discard)
        .map_err(git_err)?
        .untracked_files(gix::status::UntrackedFiles::Files);
    let iter = platform
        .into_iter(Vec::<gix::bstr::BString>::new())
        .map_err(git_err)?;

    let mut merged: BTreeMap<String, (Option<ChangeState>, Option<ChangeState>)> = BTreeMap::new();
    let mut truncated = false;
    for item in iter {
        let Ok(item) = item else { continue };
        let (rela, staged, unstaged) = match &item {
            Item::IndexWorktree(iw) => {
                let Some(summary) = iw.summary() else {
                    continue;
                };
                (
                    iw.rela_path().to_str_lossy().into_owned(),
                    None,
                    Some(from_summary(summary)),
                )
            }
            Item::TreeIndex(change) => (
                change.location().to_str_lossy().into_owned(),
                Some(from_tree_index(change)),
                None,
            ),
        };
        let Some(path) = project_relative(&rela, &prefix) else {
            continue;
        };
        if !merged.contains_key(&path) && merged.len() >= MAX_CHANGES {
            truncated = true;
            continue;
        }
        let slot = merged.entry(path).or_default();
        slot.0 = slot.0.or(staged);
        slot.1 = slot.1.or(unstaged);
    }

    Ok(GitStatus {
        is_repo: true,
        branch,
        detached,
        head: head_id,
        changes: merged
            .into_iter()
            .map(|(path, (staged, unstaged))| GitChange {
                path,
                staged,
                unstaged,
            })
            .collect(),
        truncated,
    })
}

/// Strip the project's offset inside the work tree; `None` if outside it.
fn project_relative(repo_rela: &str, prefix: &Path) -> Option<String> {
    if prefix.as_os_str().is_empty() {
        return Some(repo_rela.to_owned());
    }
    let prefix = crate::paths::to_slash(prefix)?;
    repo_rela
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_relative_strips_the_prefix() {
        assert_eq!(
            project_relative("a/b.txt", Path::new("")),
            Some("a/b.txt".to_owned())
        );
        assert_eq!(
            project_relative("papers/x/a.typ", Path::new("papers/x")),
            Some("a.typ".to_owned())
        );
        assert_eq!(
            project_relative("papers/xy/a.typ", Path::new("papers/x")),
            None
        );
        assert_eq!(project_relative("other/a.typ", Path::new("papers/x")), None);
    }

    #[test]
    fn a_plain_directory_is_not_a_repo() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path().canonicalize().expect("canon");
        let status = status(&root).expect("status");
        assert!(!status.is_repo);
    }
}
