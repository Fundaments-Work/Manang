//! Share and get updates: the plain-language face of push and pull.
//!
//! * `remote-get` / `remote-set` read and write the project's one remote
//!   (`origin`, or the first remote that exists);
//! * `sync-status` compares HEAD with the remote-tracking branch using only
//!   local data (no network) unless the caller asks to refresh;
//! * `share` pushes the current branch and never forces;
//! * `get-updates` fetches, then fast-forwards, or merges when that is clean.
//!   A merge that would conflict stops before the work tree is touched and is
//!   reported as data, not as an error.
//!
//! Authentication is whatever the system git already has (credential helpers,
//! ssh-agent). Nothing is prompted for and nothing is stored here.

use serde::Serialize;

use crate::ctx::Ctx;
use crate::error::GitError;
use crate::runner::{Runner, SYNC_TIMEOUT};
use crate::status;
use crate::versions::save_version_at;
use crate::writes::{config_value, require_identity};

const DEFAULT_REMOTE: &str = "origin";

/// How `get-updates` treats a branch that has both local and remote changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    FfOnly,
    Merge,
}

impl Strategy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ff-only" => Some(Self::FfOnly),
            "merge" => Some(Self::Merge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteInfo {
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncStatus {
    /// `no-remote`, `up-to-date`, `ahead`, `behind` or `diverged`.
    pub state: &'static str,
    pub remote: Option<String>,
    pub branch: Option<String>,
    /// Saved versions not on the remote yet (every version when never shared).
    pub ahead: usize,
    /// Versions on the remote that are not here.
    pub behind: usize,
    /// The remote has no copy of this branch yet.
    pub never_shared: bool,
    /// Files with changes that are not saved as a version.
    pub uncommitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shared {
    pub remote: String,
    pub branch: String,
    pub pushed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Updated {
    /// `up-to-date`, `fast-forwarded`, `merged`, `diverged` or `conflicts`.
    pub result: &'static str,
    /// Files that conflict (only for `conflicts`).
    pub files: Vec<String>,
    /// Versions received.
    pub received: usize,
    /// A version saved from unsaved changes before updating.
    pub safety_version: Option<String>,
}

fn remotes(run: &Runner) -> Result<Vec<String>, GitError> {
    Ok(run
        .run_ok(&["remote"])?
        .stdout_text()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

fn pick_remote(known: &[String]) -> Option<String> {
    known
        .iter()
        .find(|k| *k == DEFAULT_REMOTE)
        .or_else(|| known.first())
        .cloned()
}

fn current_branch(run: &Runner) -> Result<Option<String>, GitError> {
    let out = run.run(&["symbolic-ref", "--short", "-q", "HEAD"])?;
    Ok(out
        .ok()
        .then(|| out.stdout_text().trim().to_owned())
        .filter(|b| !b.is_empty()))
}

fn rev_exists(run: &Runner, rev: &str) -> Result<bool, GitError> {
    Ok(run.run(&["rev-parse", "--verify", "-q", rev])?.ok())
}

fn has_commits(run: &Runner) -> Result<bool, GitError> {
    rev_exists(run, "HEAD")
}

/// `(ahead, behind)` of HEAD relative to `other`.
fn counts(run: &Runner, other: &str) -> Result<(usize, usize), GitError> {
    let range = format!("HEAD...{other}");
    let out = run.run_ok(&["rev-list", "--left-right", "--count", &range])?;
    let text = out.stdout_text();
    let mut it = text.split_whitespace();
    let ahead = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let behind = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    Ok((ahead, behind))
}

fn commit_count(run: &Runner, rev: &str) -> Result<usize, GitError> {
    let out = run.run_ok(&["rev-list", "--count", rev])?;
    Ok(out.stdout_text().trim().parse().unwrap_or(0))
}

/// Refuse values git could mistake for an option, an `ext::` transport or a
/// multi-line setting.
fn check_url(url: &str) -> Result<&str, GitError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(GitError::BadRemoteUrl("the address is empty".to_owned()));
    }
    if url.starts_with('-')
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
        || url.to_ascii_lowercase().starts_with("ext:")
    {
        return Err(GitError::BadRemoteUrl(
            "that does not look like a repository address; use an https:// or ssh address"
                .to_owned(),
        ));
    }
    Ok(url)
}

pub fn remote_get(ctx: &Ctx) -> Result<RemoteInfo, GitError> {
    let run = Runner::new(ctx.root.path());
    let Some(name) = pick_remote(&remotes(&run)?) else {
        return Ok(RemoteInfo {
            name: None,
            url: None,
        });
    };
    let url = run
        .run_ok(&["remote", "get-url", &name])?
        .stdout_text()
        .trim()
        .to_owned();
    Ok(RemoteInfo {
        name: Some(name),
        url: Some(url),
    })
}

pub fn remote_set(ctx: &Ctx, url: &str) -> Result<RemoteInfo, GitError> {
    let url = check_url(url)?;
    let run = Runner::new(ctx.root.path());
    match pick_remote(&remotes(&run)?) {
        Some(name) => run.run_ok(&["remote", "set-url", &name, url])?,
        None => run.run_ok(&["remote", "add", DEFAULT_REMOTE, url])?,
    };
    remote_get(ctx)
}

fn unsaved(ctx: &Ctx) -> Result<usize, GitError> {
    Ok(status::changes(ctx)?
        .changes
        .iter()
        .filter(|c| c.staged.is_some() || c.unstaged.is_some())
        .count())
}

pub fn sync_status(ctx: &Ctx, refresh: bool) -> Result<SyncStatus, GitError> {
    let run = Runner::new(ctx.root.path());
    let uncommitted = unsaved(ctx)?;
    let branch = current_branch(&run)?;
    let mut report = SyncStatus {
        state: "no-remote",
        remote: None,
        branch: branch.clone(),
        ahead: 0,
        behind: 0,
        never_shared: false,
        uncommitted,
    };
    let Some(remote) = pick_remote(&remotes(&run)?) else {
        return Ok(report);
    };
    if refresh {
        fetch(ctx, &remote)?;
    }
    report.state = "up-to-date";
    report.remote = Some(remote.clone());
    let Some(branch) = branch else {
        return Ok(report);
    };
    if !has_commits(&run)? {
        return Ok(report);
    }
    let tracking = format!("refs/remotes/{remote}/{branch}");
    if rev_exists(&run, &tracking)? {
        let (ahead, behind) = counts(&run, &tracking)?;
        report.ahead = ahead;
        report.behind = behind;
        report.state = match (ahead, behind) {
            (0, 0) => "up-to-date",
            (_, 0) => "ahead",
            (0, _) => "behind",
            _ => "diverged",
        };
    } else {
        report.never_shared = true;
        report.ahead = commit_count(&run, "HEAD")?;
        report.state = "ahead";
    }
    Ok(report)
}

/// Turn git's stderr into something a writer can act on.
fn friendly(command: &str, code: i32, stderr: String) -> GitError {
    let lower = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has(&["[rejected]", "non-fast-forward", "fetch first"]) {
        GitError::ShareRejected
    } else if has(&[
        "authentication failed",
        "permission denied",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "access denied",
        "publickey",
        "invalid username or password",
    ]) {
        GitError::AuthFailed
    } else if has(&[
        "could not resolve host",
        "could not read from remote",
        "does not appear to be a git repository",
        "repository not found",
        "unable to access",
        "connection refused",
        "connection timed out",
        "network is unreachable",
        "no such file or directory",
    ]) {
        GitError::RemoteUnreachable
    } else {
        GitError::CommandFailed {
            command: command.to_owned(),
            code,
            stderr,
        }
    }
}

/// Run a network command: never prompts, ssh stays non-interactive.
fn run_net(ctx: &Ctx, command: &str, rest: &[&str]) -> Result<(), GitError> {
    let mut args: Vec<&str> = Vec::new();
    if config_value(ctx, "core.sshCommand").is_none() {
        args.extend(["-c", "core.sshCommand=ssh -o BatchMode=yes"]);
    }
    args.extend(rest);
    let out = Runner::new(ctx.root.path())
        .timeout(SYNC_TIMEOUT)
        .run(&args)?;
    if out.ok() {
        Ok(())
    } else {
        Err(friendly(command, out.code, out.stderr))
    }
}

fn fetch(ctx: &Ctx, remote: &str) -> Result<(), GitError> {
    run_net(
        ctx,
        "fetch",
        &["fetch", "--no-recurse-submodules", "--prune", remote],
    )
}

fn require_remote(run: &Runner) -> Result<String, GitError> {
    pick_remote(&remotes(run)?).ok_or(GitError::NoRemote)
}

pub fn share(ctx: &Ctx) -> Result<Shared, GitError> {
    let run = Runner::new(ctx.root.path());
    let remote = require_remote(&run)?;
    let Some(branch) = current_branch(&run)? else {
        return Err(GitError::BadArgs(
            "there is no branch to share; switch to a branch first".to_owned(),
        ));
    };
    if !has_commits(&run)? {
        return Err(GitError::NothingToShare);
    }
    let pushed = sync_status(ctx, false)?.ahead;
    let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
    run_net(
        ctx,
        "push",
        &[
            "push",
            "--no-recurse-submodules",
            "--set-upstream",
            &remote,
            &spec,
        ],
    )?;
    Ok(Shared {
        remote,
        branch,
        pushed,
    })
}

/// Files that would conflict if `other` were merged into HEAD, found without
/// touching the work tree or the index. Empty when the merge is clean.
fn conflicts_with(run: &Runner, other: &str) -> Result<Vec<String>, GitError> {
    if !run.run(&["merge-base", "HEAD", other])?.ok() {
        return Err(GitError::UnrelatedHistories);
    }
    let out = run.run(&[
        "merge-tree",
        "--write-tree",
        "--name-only",
        "--no-messages",
        "HEAD",
        other,
    ])?;
    match out.code {
        0 => Ok(Vec::new()),
        1 => {
            let mut files: Vec<String> = out
                .stdout_text()
                .lines()
                .skip(1)
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
            files.sort();
            files.dedup();
            Ok(files)
        }
        code => Err(GitError::CommandFailed {
            command: "merge-tree".to_owned(),
            code,
            stderr: out.stderr,
        }),
    }
}

fn outcome(result: &'static str, received: usize) -> Updated {
    Updated {
        result,
        files: Vec::new(),
        received,
        safety_version: None,
    }
}

pub fn get_updates(ctx: &Ctx, strategy: Strategy) -> Result<Updated, GitError> {
    let run = Runner::new(ctx.root.path());
    let remote = require_remote(&run)?;
    let Some(branch) = current_branch(&run)? else {
        return Err(GitError::BadArgs(
            "there is no branch to update; switch to a branch first".to_owned(),
        ));
    };
    fetch(ctx, &remote)?;
    let tracking = format!("refs/remotes/{remote}/{branch}");
    if !rev_exists(&run, &tracking)? {
        return Ok(outcome("up-to-date", 0));
    }
    let unborn = !has_commits(&run)?;
    let (ahead, behind) = if unborn {
        (0, commit_count(&run, &tracking)?)
    } else {
        counts(&run, &tracking)?
    };
    if behind == 0 {
        return Ok(outcome("up-to-date", 0));
    }
    if ahead > 0 && strategy == Strategy::FfOnly {
        return Ok(outcome("diverged", behind));
    }

    if strategy == Strategy::FfOnly {
        // git refuses when unsaved edits touch files the update changes and
        // leaves everything as it was.
        let moved = run.run(&["merge", "--ff-only", &tracking])?;
        if !moved.ok() {
            return Err(GitError::DirtyTree(moved.stderr));
        }
        return Ok(outcome("fast-forwarded", behind));
    }

    // Merge strategy: unsaved work becomes a version first, so nothing is lost
    // and the branch can move. The work tree is untouched by a version.
    let dirty = status::changes(ctx)?
        .changes
        .iter()
        .any(|c| c.staged.is_some() || c.unstaged.is_some());
    let mut safety_version = None;
    if dirty && !unborn {
        require_identity(ctx)?;
        match save_version_at(ctx, Some("Before getting updates"), None, 0) {
            Ok(saved) => safety_version = Some(saved.sha),
            Err(GitError::NothingToSave) => {}
            Err(e) => return Err(e),
        }
    }
    if ahead > 0 || safety_version.is_some() {
        let files = conflicts_with(&run, &tracking)?;
        if !files.is_empty() {
            return Ok(Updated {
                result: "conflicts",
                files,
                received: behind,
                safety_version,
            });
        }
        require_identity(ctx)?;
        let merged = run.run(&[
            "merge",
            "--no-edit",
            "--no-ff",
            "-m",
            "Get updates",
            &tracking,
        ])?;
        if !merged.ok() {
            let _ = run.run(&["merge", "--abort"]);
            return Err(GitError::CommandFailed {
                command: "merge".to_owned(),
                code: merged.code,
                stderr: merged.stderr,
            });
        }
        return Ok(Updated {
            result: "merged",
            files: Vec::new(),
            received: behind,
            safety_version,
        });
    }
    let moved = run.run(&["merge", "--ff-only", &tracking])?;
    if !moved.ok() {
        return Err(GitError::DirtyTree(moved.stderr));
    }
    Ok(Updated {
        result: "fast-forwarded",
        files: Vec::new(),
        received: behind,
        safety_version: None,
    })
}
