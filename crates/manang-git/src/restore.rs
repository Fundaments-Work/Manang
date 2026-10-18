//! Restore files from an earlier version into the work tree.
//!
//! History is never rewritten and nothing is deleted: only paths that exist
//! at the revision are written, and every file about to be overwritten is
//! first copied to `<git-dir>/manang-trash/` (the same safety copy
//! `git.discard` makes). Uncommitted changes to files that would be replaced
//! are refused unless the caller asks for a safety version first.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::ctx::Ctx;
use crate::error::GitError;
use crate::runner::Runner;
use crate::status;
use crate::versions::{resolve_commit, save_version_at};
use crate::writes::{MAX_PATHS, trash_copy};

/// Paths handed to one `git restore` call.
const CHUNK: usize = 400;

#[derive(Debug, Clone, Copy)]
pub struct RestoreRequest<'a> {
    pub rev: &'a str,
    pub paths: &'a [String],
    pub whole: bool,
    pub save_first: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Restored {
    /// Files whose content was replaced (or recreated) from the revision.
    pub restored: Vec<String>,
    /// Files that exist now but not at the revision; left exactly as they are.
    pub untouched_new: Vec<String>,
    /// Commit made from the current changes before restoring, when asked.
    pub safety_version: Option<String>,
    /// Where the overwritten files were copied.
    pub trash: Option<String>,
}

/// `(path, blob id)` of every file in the revision's tree under the project.
fn revision_files(run: &Runner, sha: &str) -> Result<Vec<(String, gix::ObjectId)>, GitError> {
    let out = run.run_ok(&["ls-tree", "-r", "-z", sha])?;
    let mut files = Vec::new();
    for entry in out.stdout.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let Some(tab) = entry.iter().position(|b| *b == b'\t') else {
            continue;
        };
        let (meta, path) = entry.split_at(tab);
        let meta = String::from_utf8_lossy(meta);
        let mut parts = meta.split(' ');
        let (Some(_mode), Some(kind), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if kind != "blob" {
            continue; // submodules are not restored
        }
        let Ok(id) = gix::ObjectId::from_hex(id.as_bytes()) else {
            continue;
        };
        let Ok(path) = std::str::from_utf8(&path[1..]) else {
            continue;
        };
        files.push((path.to_owned(), id));
    }
    Ok(files)
}

/// The work-tree file differs from the blob (or is a symlink with another
/// target). `None` when the file does not exist now.
fn differs(ctx: &Ctx, rel: &str, id: gix::ObjectId) -> Result<Option<bool>, GitError> {
    let full = ctx.root.resolve_entry(rel)?;
    let Ok(meta) = fs::symlink_metadata(&full) else {
        return Ok(None);
    };
    let blob = ctx.repo.find_object(id).map_err(GitError::backend)?;
    if meta.is_dir() {
        return Ok(Some(true));
    }
    let current = if meta.file_type().is_symlink() {
        fs::read_link(&full)
            .map_err(|e| GitError::io(format!("reading {rel}"), e))?
            .to_string_lossy()
            .into_owned()
            .into_bytes()
    } else {
        if meta.len() != blob.data.len() as u64 {
            return Ok(Some(true));
        }
        fs::read(&full).map_err(|e| GitError::io(format!("reading {rel}"), e))?
    };
    Ok(Some(current != blob.data[..]))
}

fn in_scope(path: &str, scope: &str) -> bool {
    path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub fn restore(ctx: &Ctx, req: &RestoreRequest<'_>) -> Result<Restored, GitError> {
    let sha = resolve_commit(ctx, req.rev)?.to_string();
    let short: String = sha.chars().take(7).collect();
    if !req.whole && req.paths.is_empty() {
        return Err(GitError::BadArgs(
            "name the `paths` to restore, or pass `whole: true`".to_owned(),
        ));
    }
    if req.paths.len() > MAX_PATHS {
        return Err(GitError::BadArgs(format!(
            "too many paths (limit {MAX_PATHS})"
        )));
    }
    let run = Runner::new(ctx.root.path());
    let at_rev = revision_files(&run, &sha)?;

    let targets: Vec<&(String, gix::ObjectId)> = if req.whole {
        at_rev.iter().collect()
    } else {
        let mut chosen = BTreeSet::new();
        for raw in req.paths {
            let rel = ctx.rel_for_write(raw)?;
            let mut found = false;
            for (i, (p, _)) in at_rev.iter().enumerate() {
                if in_scope(p, &rel) {
                    chosen.insert(i);
                    found = true;
                }
            }
            if !found {
                return Err(GitError::PathNotInRev(rel));
            }
        }
        chosen.into_iter().filter_map(|i| at_rev.get(i)).collect()
    };

    // Files whose current content would be replaced.
    let mut replace: Vec<(&str, bool)> = Vec::new(); // (path, exists now)
    for (path, id) in &targets {
        match differs(ctx, path, *id)? {
            Some(true) => replace.push((path, true)),
            None => replace.push((path, false)),
            Some(false) => {}
        }
    }

    // Uncommitted work among files that exist now and would be overwritten.
    let dirty_now: BTreeSet<String> = status::changes(ctx)?
        .changes
        .into_iter()
        .filter(|c| c.staged.is_some() || c.unstaged.is_some())
        .map(|c| c.path)
        .collect();
    let dirty: Vec<String> = replace
        .iter()
        .filter(|(p, exists)| *exists && dirty_now.contains(*p))
        .map(|(p, _)| (*p).to_owned())
        .collect();
    if !dirty.is_empty() && !req.save_first {
        return Err(GitError::DirtyPaths(dirty));
    }

    // Safety copies first: a failure aborts before anything is replaced.
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (path, exists) in &replace {
        if *exists {
            copies.push((ctx.root.resolve_entry(path)?, PathBuf::from(path)));
        }
    }
    let trash = trash_copy(ctx, &copies)?;

    let mut safety_version = None;
    if !dirty.is_empty() {
        let name = format!("Before restoring {short}");
        match save_version_at(ctx, Some(&name), None, 0) {
            Ok(saved) => safety_version = Some(saved.sha),
            Err(GitError::NothingToSave) => {}
            Err(e) => return Err(e),
        }
    }

    let names: Vec<&str> = replace.iter().map(|(p, _)| *p).collect();
    for chunk in names.chunks(CHUNK) {
        let mut args = vec!["restore", "--source", sha.as_str(), "--worktree", "--"];
        args.extend(chunk);
        run.run_ok(&args)?;
    }

    let untouched_new = if req.whole {
        let known: BTreeSet<&str> = at_rev.iter().map(|(p, _)| p.as_str()).collect();
        let listed = run.run_ok(&[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])?;
        let mut extra: BTreeSet<String> = BTreeSet::new();
        for p in listed.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            if let Ok(p) = std::str::from_utf8(p)
                && !known.contains(p)
            {
                extra.insert(p.to_owned());
            }
        }
        extra.into_iter().collect()
    } else {
        Vec::new()
    };

    Ok(Restored {
        restored: names.iter().map(|p| (*p).to_owned()).collect(),
        untouched_new,
        safety_version,
        trash,
    })
}
