//! Mutating operations. All of them drive the system `git` through
//! [`Runner`] (see ADR 0005 for why), after validating every path against
//! the project root.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use manang_project::git::ChangeState;
use manang_project::lexical_relative;
use serde::Serialize;

use crate::ctx::{Ctx, has_dot_git};
use crate::error::GitError;
use crate::runner::{MAX_SYNC_TIMEOUT, Runner, SYNC_TIMEOUT};
use crate::status;

pub const MAX_PATHS: usize = 10_000;
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
/// Most bytes saved to the discard trash in one call.
pub const MAX_TRASH_BYTES: u64 = 256 * 1024 * 1024;
const OUTPUT_TAIL: usize = 16 * 1024;

fn is_root(raw: &str) -> bool {
    lexical_relative(raw).is_ok_and(|p| p.as_os_str().is_empty())
}

/// Validate caller paths into pathspecs. `.` (the project root) is allowed
/// when `allow_root`.
fn pathspecs(ctx: &Ctx, paths: &[String], allow_root: bool) -> Result<Vec<String>, GitError> {
    if paths.is_empty() {
        return Err(GitError::BadArgs("`paths` must not be empty".to_owned()));
    }
    if paths.len() > MAX_PATHS {
        return Err(GitError::BadArgs(format!(
            "too many paths (limit {MAX_PATHS})"
        )));
    }
    let mut out = Vec::with_capacity(paths.len());
    for raw in paths {
        if is_root(raw) {
            if !allow_root {
                return Err(GitError::BadArgs(
                    "the project root cannot be used here".to_owned(),
                ));
            }
            ctx.rel(raw)?;
            out.push(".".to_owned());
        } else {
            out.push(ctx.rel_for_write(raw)?);
        }
    }
    Ok(out)
}

fn runner(ctx: &Ctx, hooks: bool) -> Runner {
    Runner::new(ctx.root.path()).hooks(hooks)
}

fn with_paths<'a>(head: &[&'a str], specs: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = head.to_vec();
    args.push("--");
    args.extend(specs.iter().map(String::as_str));
    args
}

pub fn stage(ctx: &Ctx, paths: &[String]) -> Result<usize, GitError> {
    let specs = pathspecs(ctx, paths, true)?;
    runner(ctx, false).run_ok(&with_paths(&["add", "--all"], &specs))?;
    Ok(specs.len())
}

pub fn unstage(ctx: &Ctx, paths: &[String]) -> Result<usize, GitError> {
    let specs = pathspecs(ctx, paths, true)?;
    let run = runner(ctx, false);
    if ctx.repo.head_id().is_ok() {
        run.run_ok(&with_paths(&["reset", "-q"], &specs))?;
    } else {
        // No HEAD to reset to: dropping index entries is the same thing.
        run.run_ok(&with_paths(
            &["rm", "--cached", "-r", "-q", "--ignore-unmatch"],
            &specs,
        ))?;
    }
    Ok(specs.len())
}

#[derive(Debug, Serialize)]
pub struct Discarded {
    pub discarded: usize,
    /// Directory holding the saved copies, `None` when nothing needed saving.
    pub trash: Option<String>,
}

fn in_scope(path: &str, scope: &str) -> bool {
    scope == "."
        || path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn unique_trash_dir(git_dir: &Path) -> Result<PathBuf, GitError> {
    let base = git_dir.join("manang-trash");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    fs::create_dir_all(&base).map_err(|e| GitError::io("creating the discard trash", e))?;
    for n in 0..1000u32 {
        let dir = base.join(format!("{stamp}-{n}"));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(GitError::io("creating the discard trash", e)),
        }
    }
    Err(GitError::Backend(
        "could not allocate a trash directory".to_owned(),
    ))
}

/// Throw away work-tree changes (and untracked files) under `paths`. Every
/// affected file is first copied to `<git-dir>/manang-trash/<stamp>/`, so
/// nothing is unrecoverable; staged changes are left alone.
pub fn discard(ctx: &Ctx, paths: &[String], confirm: bool) -> Result<Discarded, GitError> {
    if !confirm {
        return Err(GitError::ConfirmRequired);
    }
    let specs = pathspecs(ctx, paths, true)?;
    let st = status::changes(ctx)?;
    let targets: Vec<_> = st
        .changes
        .iter()
        .filter(|c| {
            c.unstaged.is_some()
                && c.unstaged != Some(ChangeState::Conflicted)
                && specs.iter().any(|s| in_scope(&c.path, s))
        })
        .collect();

    // 1. Save first; a failure here aborts before anything is destroyed.
    let mut total = 0u64;
    let mut staged_copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    for t in &targets {
        let src = ctx.root.resolve_entry(&t.path)?;
        let Ok(meta) = fs::symlink_metadata(&src) else {
            continue; // deleted in the work tree: git can restore it
        };
        if meta.is_dir() {
            continue;
        }
        total += meta.len();
        if total > MAX_TRASH_BYTES {
            return Err(GitError::TooLarge {
                what: "discarded files".to_owned(),
                size: total,
                limit: MAX_TRASH_BYTES,
            });
        }
        staged_copies.push((src, PathBuf::from(&t.path)));
    }
    let mut trash = None;
    if !staged_copies.is_empty() {
        let dir = unique_trash_dir(&ctx.git_dir())?;
        for (src, rel) in &staged_copies {
            let dest = dir.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| GitError::io("saving to trash", e))?;
            }
            if fs::symlink_metadata(src).is_ok_and(|m| m.file_type().is_symlink()) {
                let target = fs::read_link(src).map_err(|e| GitError::io("saving to trash", e))?;
                fs::write(&dest, target.to_string_lossy().as_bytes())
                    .map_err(|e| GitError::io("saving to trash", e))?;
            } else {
                fs::copy(src, &dest).map_err(|e| GitError::io("saving to trash", e))?;
            }
        }
        trash = Some(dir.display().to_string());
    }

    // 2. Destroy: untracked files first, then restore tracked ones.
    let run = runner(ctx, false);
    if targets
        .iter()
        .any(|t| t.unstaged == Some(ChangeState::Untracked))
    {
        run.run_ok(&with_paths(&["clean", "-f", "-d", "-q"], &specs))?;
    }
    let tracked = run.run_ok(&with_paths(&["ls-files", "-z"], &specs))?;
    if !tracked.stdout.is_empty() {
        run.run_ok(&with_paths(&["restore"], &specs))?;
    }
    Ok(Discarded {
        discarded: targets.len(),
        trash,
    })
}

fn config_value(ctx: &Ctx, key: &str) -> Option<String> {
    ctx.repo
        .config_snapshot()
        .string(key)
        .map(|v| v.to_string())
        .filter(|v| !v.trim().is_empty())
}

/// Fail with an actionable message unless git has a name and email.
pub fn require_identity(ctx: &Ctx) -> Result<(), GitError> {
    if config_value(ctx, "user.name").is_some() && config_value(ctx, "user.email").is_some() {
        Ok(())
    } else {
        Err(GitError::MissingIdentity)
    }
}

#[derive(Debug, Serialize)]
pub struct Committed {
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub amended: bool,
}

pub fn commit(ctx: &Ctx, message: &str, amend: bool, hooks: bool) -> Result<Committed, GitError> {
    if message.trim().is_empty() {
        return Err(GitError::EmptyMessage);
    }
    if message.len() > MAX_MESSAGE_BYTES || message.contains('\0') {
        return Err(GitError::BadArgs("invalid commit message".to_owned()));
    }
    require_identity(ctx)?;
    let run = runner(ctx, hooks);
    let has_head = ctx.repo.head_id().is_ok();
    if amend {
        if !has_head {
            return Err(GitError::AmendUnborn);
        }
        let published = run.run_ok(&[
            "for-each-ref",
            "--count=1",
            "--contains",
            "HEAD",
            "--format=%(refname)",
            "refs/remotes",
        ])?;
        if !published.stdout.is_empty() {
            return Err(GitError::AmendPublished);
        }
    } else if !ctx.git_dir().join("MERGE_HEAD").exists() {
        let cached = run.run(&["diff", "--cached", "--quiet"])?;
        if cached.code == 0 {
            return Err(GitError::NothingToCommit);
        }
    }
    let mut args = vec![
        "-c",
        "user.useConfigOnly=true",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--cleanup=whitespace",
    ];
    if !hooks {
        args.push("--no-verify");
    }
    if amend {
        args.push("--amend");
    }
    args.push("-m");
    args.push(message);
    let out = run.run(&args)?;
    if !out.ok() {
        let text = format!("{}\n{}", out.stderr, out.stdout_text());
        if text.contains("Author identity unknown") || text.contains("unable to auto-detect") {
            return Err(GitError::MissingIdentity);
        }
        return Err(GitError::CommandFailed {
            command: "commit".to_owned(),
            code: out.code,
            stderr: out.stderr,
        });
    }
    let hash = run
        .run_ok(&["rev-parse", "HEAD"])?
        .stdout_text()
        .trim()
        .to_owned();
    Ok(Committed {
        short: hash.chars().take(7).collect(),
        subject: message.lines().next().unwrap_or("").trim().to_owned(),
        hash,
        amended: amend,
    })
}

fn valid_branch_shape(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('-')
        && !name.contains(['\0', '\n', '\r'])
}

pub fn checkout_branch(ctx: &Ctx, name: &str, create: bool) -> Result<(), GitError> {
    if !valid_branch_shape(name) {
        return Err(GitError::BadBranchName(name.chars().take(80).collect()));
    }
    let run = runner(ctx, false);
    let refname = format!("refs/heads/{name}");
    if !run.run(&["check-ref-format", &refname])?.ok() {
        return Err(GitError::BadBranchName(name.to_owned()));
    }
    let args: Vec<&str> = if create {
        vec!["switch", "-q", "-c", name]
    } else {
        vec!["switch", "-q", name]
    };
    let out = run.run(&args)?;
    if out.ok() {
        return Ok(());
    }
    let text = &out.stderr;
    if text.contains("would be overwritten") || text.contains("commit your changes or stash") {
        return Err(GitError::DirtyTree(text.clone()));
    }
    Err(GitError::CommandFailed {
        command: "switch".to_owned(),
        code: out.code,
        stderr: out.stderr,
    })
}

pub fn init(ctx_root: &manang_project::ProjectRoot) -> Result<(), GitError> {
    if has_dot_git(ctx_root.path()) {
        return Err(GitError::AlreadyRepo(ctx_root.path().display().to_string()));
    }
    Runner::new(ctx_root.path()).run_ok(&["init", "-q"])?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    Fetch,
    Pull,
    Push,
}

impl SyncAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fetch" => Some(Self::Fetch),
            "pull" => Some(Self::Pull),
            "push" => Some(Self::Push),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Synced {
    pub action: &'static str,
    pub remote: Option<String>,
    pub output: String,
}

fn valid_remote_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn tail(mut text: String) -> String {
    if text.len() > OUTPUT_TAIL {
        let mut cut = text.len() - OUTPUT_TAIL;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        text = text[cut..].to_owned();
    }
    text
}

/// Fetch, fast-forward pull, or plain push through the system git. This is
/// the only code path that touches the network, and only when the user
/// invoked `git.sync`. Never forces; never prompts.
pub fn sync(
    ctx: &Ctx,
    action: SyncAction,
    remote: Option<&str>,
    timeout_secs: Option<u64>,
    hooks: bool,
) -> Result<Synced, GitError> {
    let timeout = timeout_secs
        .map(|s| Duration::from_secs(s.max(5)))
        .unwrap_or(SYNC_TIMEOUT)
        .min(MAX_SYNC_TIMEOUT);
    let known: Vec<String> = ctx
        .repo
        .remote_names()
        .into_iter()
        .map(|n| n.to_string())
        .collect();
    let head = ctx.repo.head().map_err(GitError::backend)?;
    let branch = head.referent_name().map(|n| n.shorten().to_string());
    let upstream_remote = branch.as_deref().and_then(|b| {
        ctx.repo
            .branch_remote_name(b, gix::remote::Direction::Fetch)
            .map(|n| n.as_bstr().to_string())
    });
    let remote = match remote {
        Some(r) => {
            if !valid_remote_name(r) || !known.iter().any(|k| k == r) {
                return Err(GitError::BadArgs(format!(
                    "`{r}` is not a configured remote; add it with `git remote add`"
                )));
            }
            Some(r.to_owned())
        }
        None => upstream_remote
            .clone()
            .filter(|r| known.contains(r))
            .or_else(|| known.iter().find(|k| *k == "origin").cloned())
            .or_else(|| known.first().cloned()),
    };
    let Some(remote_name) = remote.clone() else {
        return Err(GitError::BadArgs(
            "this repository has no remote; add one with `git remote add origin <url>`".to_owned(),
        ));
    };

    let mut run = Runner::new(ctx.root.path()).timeout(timeout).hooks(hooks);
    let mut pre: Vec<String> = vec!["-c".into(), "push.default=simple".into()];
    if config_value(ctx, "core.sshCommand").is_none() {
        // ssh would otherwise ask for passphrases or host-key trust.
        pre.push("-c".into());
        pre.push("core.sshCommand=ssh -o BatchMode=yes".into());
    }
    let mut args: Vec<String> = pre;
    match action {
        SyncAction::Fetch => {
            args.extend(
                ["fetch", "--no-recurse-submodules", remote_name.as_str()].map(String::from),
            );
        }
        SyncAction::Pull => {
            args.extend(
                [
                    "pull",
                    "--ff-only",
                    "--no-rebase",
                    "--no-recurse-submodules",
                    remote_name.as_str(),
                ]
                .map(String::from),
            );
        }
        SyncAction::Push => {
            let Some(branch) = branch else {
                return Err(GitError::BadArgs(
                    "cannot push a detached HEAD; check out a branch first".to_owned(),
                ));
            };
            let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
            args.extend(["push", "--no-recurse-submodules"].map(String::from));
            if upstream_remote.is_none() {
                args.push("--set-upstream".into());
            }
            args.push(remote_name.clone());
            args.push(spec);
        }
    }
    run = run.timeout(timeout);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run.run(&refs)?;
    let text = tail(
        format!("{}\n{}", out.stderr, out.stdout_text())
            .trim()
            .to_owned(),
    );
    if !out.ok() {
        return Err(GitError::CommandFailed {
            command: match action {
                SyncAction::Fetch => "fetch",
                SyncAction::Pull => "pull",
                SyncAction::Push => "push",
            }
            .to_owned(),
            code: out.code,
            stderr: text,
        });
    }
    Ok(Synced {
        action: match action {
            SyncAction::Fetch => "fetch",
            SyncAction::Pull => "pull",
            SyncAction::Push => "push",
        },
        remote,
        output: text,
    })
}
