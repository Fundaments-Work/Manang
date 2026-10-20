//! Disk bridge: applies a remote project's file operations to the local disk.
//!
//! [`DiskBridge`] is the only writer of files that are *not* open in a tab. It
//! is synchronous and clock-driven (no threads, no timers of its own): the
//! owner (the session thread, package P4) calls the operations as remote
//! changes arrive and calls [`DiskBridge::tick`] now and then. A fake
//! [`Clock`] makes it fully testable.
//!
//! # Rules
//!
//! * Every path goes through [`PathPolicy::check_write`] first (see
//!   [`crate::pathsec`]); the check is repeated when a debounced write runs.
//! * Writes are atomic (`manang_project::fsops::atomic_write`). Text changes
//!   are **debounced**: [`DiskBridge::queue_text`] restarts a quiet period of
//!   [`DEBOUNCE_MS`] (500 ms) and only the last text is written.
//! * Files **open in a tab** ([`DiskBridge::set_open`]) are never written
//!   here: the web view's autosave stays their single writer. The bridge
//!   answers [`WriteOutcome::DirtyOpen`] and remembers it; when the tab
//!   closes, [`DiskBridge::close`] says whether the owner must flush (queue
//!   the current shared text) and performs a deferred delete.
//! * Nothing is ever removed: deletes and replaced files go to the project's
//!   recoverable trash `.manang/trash/<id>/<path>` (`fsops::trash`).
//! * Echo suppression: before every write the bridge records what it is about
//!   to cause on disk (`expected[path]` = content hash, gone, or directory)
//!   for [`ECHO_TTL_MS`]. The file watcher reports our own writes back;
//!   [`DiskBridge::is_echo`] lets the tree bridge drop them.
//! * A debounced write that would replace content somebody else put on disk
//!   since the bridge last wrote the file first copies that content to the
//!   trash, so a racing local edit is recoverable.
//!
//! The module also holds the join classification
//! ([`classify_local`]), which produces the data of the `live.plan` event.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use manang_live::blob::blob_hash;
use manang_live::manifest::ManifestEntry;
use manang_live::project::NodeKind;
use manang_project::fsops;
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

use crate::pathsec::{PathError, PathPolicy};

/// Quiet period before a debounced text write.
pub const DEBOUNCE_MS: u64 = 500;
/// How long an expected-write record suppresses watcher events.
pub const ECHO_TTL_MS: u64 = 10_000;
/// Most files a join scan will look at.
pub const MAX_SCAN_FILES: usize = 65_536;

/// A SHA-256 content hash.
pub type Hash = [u8; 32];

/// Source of time in milliseconds (monotonic; the origin does not matter).
pub trait Clock {
    /// Milliseconds since an arbitrary fixed origin.
    fn now_ms(&self) -> u64;
}

/// The real clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A clock starting now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// A clock moved by hand; clones share the same time.
#[derive(Debug, Clone, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    /// A clock at 0 ms.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Move time forward.
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Errors of the disk bridge.
#[derive(Debug, Error)]
pub enum DiskError {
    /// The path (or size) was refused by the security policy.
    #[error(transparent)]
    Path(#[from] PathError),
    /// A file system operation failed.
    #[error("{0}")]
    Fs(String),
    /// A file was expected where a directory is, or the other way round.
    #[error("`{0}` is not the kind of entry expected")]
    WrongKind(String),
    /// The source of a rename does not exist.
    #[error("`{0}` does not exist")]
    NotFound(String),
    /// The folder holds more files than a join scan will look at.
    #[error("too many files to scan")]
    TooMany,
}

fn fs_err(e: impl std::fmt::Display) -> DiskError {
    DiskError::Fs(e.to_string())
}

/// SHA-256 of `bytes`.
#[must_use]
pub fn sha256(bytes: &[u8]) -> Hash {
    blob_hash(bytes)
}

/// What the watcher (or a scan) saw at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// The path no longer exists.
    Gone,
    /// A file with this content hash.
    Content(Hash),
    /// A directory.
    Dir,
}

#[derive(Debug, Clone, Copy)]
struct Echo {
    state: Observed,
    until_ms: u64,
}

#[derive(Debug)]
struct Pending {
    text: Vec<u8>,
    due_ms: u64,
}

/// Result of asking for a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    /// Will be written when [`DiskBridge::tick`] runs after `due_ms`.
    Queued {
        /// Clock time at which the quiet period ends.
        due_ms: u64,
    },
    /// The file is open in a tab: not written; flush it when the tab closes.
    DirtyOpen,
    /// Written now.
    Written {
        /// Hash of the content on disk.
        sha256: Hash,
    },
    /// The disk already had exactly this content.
    Unchanged,
}

/// A debounced write that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flushed {
    /// Project-relative path.
    pub path: String,
    /// Hash of the content now on disk.
    pub sha256: Hash,
    /// Where the previous, locally edited content was saved, if it had been.
    pub backup: Option<String>,
}

/// Outcome of [`DiskBridge::tick`] / [`DiskBridge::flush_all`].
#[derive(Debug, Default)]
pub struct TickReport {
    /// Writes that ran.
    pub written: Vec<Flushed>,
    /// Writes that failed (dropped; the owner reports them).
    pub failed: Vec<(String, DiskError)>,
}

/// Outcome of a remote rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    /// The destination existed and was moved here first (host wins).
    pub trashed: Option<String>,
    /// The source was already gone and the destination present: nothing to do.
    pub noop: bool,
    /// The renamed file is open in a tab (the tab must follow the new path).
    pub open: bool,
}

/// Outcome of a remote delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deleted {
    /// Moved to the trash at this project-relative path.
    Trashed(String),
    /// Nothing was there.
    AlreadyGone,
    /// Open in a tab: the delete is deferred until [`DiskBridge::close`].
    DeferredOpen,
}

/// Outcome of [`DiskBridge::close`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Closed {
    /// Remote changes arrived while the tab was open: queue the shared text.
    pub flush_needed: bool,
    /// A deferred remote delete ran: the trash path.
    pub trashed: Option<String>,
}

/// Applies remote operations to a project folder.
#[derive(Debug)]
pub struct DiskBridge<C: Clock> {
    policy: PathPolicy,
    clock: C,
    debounce_ms: u64,
    pending: BTreeMap<String, Pending>,
    open: BTreeSet<String>,
    dirty_open: BTreeSet<String>,
    deferred_delete: BTreeSet<String>,
    echo: BTreeMap<String, Echo>,
    last_written: BTreeMap<String, Hash>,
}

impl<C: Clock> DiskBridge<C> {
    /// A bridge over `policy`'s project.
    pub fn new(policy: PathPolicy, clock: C) -> Self {
        Self {
            policy,
            clock,
            debounce_ms: DEBOUNCE_MS,
            pending: BTreeMap::new(),
            open: BTreeSet::new(),
            dirty_open: BTreeSet::new(),
            deferred_delete: BTreeSet::new(),
            echo: BTreeMap::new(),
            last_written: BTreeMap::new(),
        }
    }

    /// The security policy (also used by the tree bridge).
    #[must_use]
    pub fn policy(&self) -> &PathPolicy {
        &self.policy
    }

    /// Mutable policy, to reload ignore rules.
    pub fn policy_mut(&mut self) -> &mut PathPolicy {
        &mut self.policy
    }

    /// Mark `path` open in a tab. A write already queued for it is turned
    /// into a "dirty, flush on close" mark: the tab's autosave is the writer
    /// from now on. Closing is [`close`](Self::close).
    pub fn set_open(&mut self, path: &str) {
        self.open.insert(path.to_owned());
        if self.pending.remove(path).is_some() {
            self.dirty_open.insert(path.to_owned());
        }
    }

    /// Whether `path` is open in a tab.
    #[must_use]
    pub fn is_open(&self, path: &str) -> bool {
        self.open.contains(path)
    }

    /// The tab of `path` closed: say what the owner owes the disk. A deferred
    /// delete runs here; a pending dirty mark is reported and cleared.
    pub fn close(&mut self, path: &str) -> Result<Closed, DiskError> {
        self.open.remove(path);
        let flush_needed = self.dirty_open.remove(path);
        let trashed = if self.deferred_delete.remove(path) {
            match self.delete(path)? {
                Deleted::Trashed(t) => Some(t),
                Deleted::AlreadyGone | Deleted::DeferredOpen => None,
            }
        } else {
            None
        };
        Ok(Closed {
            flush_needed: flush_needed && trashed.is_none(),
            trashed,
        })
    }

    /// Whether a remote change to `path` is waiting (queued, or owed to an
    /// open tab). The disk is behind the shared state for such a path.
    #[must_use]
    pub fn is_behind(&self, path: &str) -> bool {
        self.pending.contains_key(path) || self.dirty_open.contains(path)
    }

    /// Queue the new shared text of a file (debounced).
    pub fn queue_text(&mut self, path: &str, text: &str) -> Result<WriteOutcome, DiskError> {
        self.queue(path, text.as_bytes().to_vec(), NodeKind::Text)
    }

    /// Write a blob (last writer wins) now, unless the file is open.
    pub fn write_blob(&mut self, path: &str, bytes: &[u8]) -> Result<WriteOutcome, DiskError> {
        self.policy.check_size(NodeKind::Blob, bytes.len() as u64)?;
        self.policy.check_write(path)?;
        if self.open.contains(path) {
            self.dirty_open.insert(path.to_owned());
            return Ok(WriteOutcome::DirtyOpen);
        }
        self.pending.remove(path);
        self.write_now(path, bytes)
    }

    fn queue(
        &mut self,
        path: &str,
        bytes: Vec<u8>,
        kind: NodeKind,
    ) -> Result<WriteOutcome, DiskError> {
        self.policy.check_size(kind, bytes.len() as u64)?;
        self.policy.check_write(path)?;
        if self.open.contains(path) {
            self.dirty_open.insert(path.to_owned());
            return Ok(WriteOutcome::DirtyOpen);
        }
        let due_ms = self.clock.now_ms() + self.debounce_ms;
        self.pending.insert(
            path.to_owned(),
            Pending {
                text: bytes,
                due_ms,
            },
        );
        Ok(WriteOutcome::Queued { due_ms })
    }

    /// Create a directory (and missing parents).
    pub fn create_dir(&mut self, path: &str) -> Result<(), DiskError> {
        let safe = self.policy.check_write(path)?;
        if safe.abs.is_dir() {
            return Ok(());
        }
        if safe.abs.exists() {
            return Err(DiskError::WrongKind(path.to_owned()));
        }
        self.expect(path, Observed::Dir);
        fs::create_dir_all(&safe.abs).map_err(fs_err)
    }

    /// Run every queued write whose quiet period is over and expire old echo
    /// records.
    pub fn tick(&mut self) -> TickReport {
        let now = self.clock.now_ms();
        self.echo.retain(|_, e| e.until_ms > now);
        let due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| p.due_ms <= now)
            .map(|(k, _)| k.clone())
            .collect();
        self.run(due)
    }

    /// Run every queued write now (leaving the session, closing the app).
    pub fn flush_all(&mut self) -> TickReport {
        let all: Vec<String> = self.pending.keys().cloned().collect();
        self.run(all)
    }

    /// Run the queued write of one path now, if any.
    pub fn flush_path(&mut self, path: &str) -> TickReport {
        self.run(vec![path.to_owned()])
    }

    fn run(&mut self, paths: Vec<String>) -> TickReport {
        let mut report = TickReport::default();
        for path in paths {
            let Some(p) = self.pending.remove(&path) else {
                continue;
            };
            if self.open.contains(&path) {
                self.dirty_open.insert(path);
                continue;
            }
            match self.write_with_backup(&path, &p.text) {
                Ok(Some(f)) => report.written.push(f),
                Ok(None) => {}
                Err(e) => report.failed.push((path, e)),
            }
        }
        report
    }

    fn write_with_backup(
        &mut self,
        path: &str,
        bytes: &[u8],
    ) -> Result<Option<Flushed>, DiskError> {
        let safe = self.policy.check_write(path)?;
        let mut backup = None;
        if let Ok(existing) = fs::read(&safe.abs) {
            let have = sha256(&existing);
            if have == sha256(bytes) {
                self.last_written.insert(path.to_owned(), have);
                return Ok(None);
            }
            if self.last_written.get(path).is_some_and(|w| *w != have) {
                backup = fsops::backup_file(self.policy.root(), path).map_err(fs_err)?;
            }
        }
        match self.write_now(path, bytes)? {
            WriteOutcome::Written { sha256 } => Ok(Some(Flushed {
                path: path.to_owned(),
                sha256,
                backup,
            })),
            _ => Ok(None),
        }
    }

    fn write_now(&mut self, path: &str, bytes: &[u8]) -> Result<WriteOutcome, DiskError> {
        let safe = self.policy.check_write(path)?;
        if safe.abs.is_dir() {
            return Err(DiskError::WrongKind(path.to_owned()));
        }
        let hash = sha256(bytes);
        if fs::read(&safe.abs).is_ok_and(|have| sha256(&have) == hash) {
            self.last_written.insert(path.to_owned(), hash);
            return Ok(WriteOutcome::Unchanged);
        }
        if let Some(parent) = safe.abs.parent() {
            fs::create_dir_all(parent).map_err(fs_err)?;
        }
        self.expect(path, Observed::Content(hash));
        if let Err(e) = fsops::atomic_write(&safe.abs, bytes) {
            self.echo.remove(path);
            return Err(fs_err(e));
        }
        self.last_written.insert(path.to_owned(), hash);
        Ok(WriteOutcome::Written { sha256: hash })
    }

    /// Rename or move `from` to `to` (a remote tree operation). If `to`
    /// exists it goes to the trash first: the host's tree wins.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<Renamed, DiskError> {
        let src = self.policy.check_write(from)?;
        let dst = self.policy.check_write(to)?;
        let open = self.open.contains(from);
        if src.abs.symlink_metadata().is_err() {
            if dst.abs.symlink_metadata().is_ok() {
                return Ok(Renamed {
                    trashed: None,
                    noop: true,
                    open,
                });
            }
            return Err(DiskError::NotFound(from.to_owned()));
        }
        let trashed = if dst.abs.symlink_metadata().is_ok() {
            self.pending.remove(to);
            self.expect(to, Observed::Gone);
            Some(fsops::trash(self.policy.root(), to).map_err(fs_err)?)
        } else {
            None
        };
        let is_dir = src.abs.is_dir();
        let hash = if is_dir {
            None
        } else {
            fs::read(&src.abs).ok().map(|b| sha256(&b))
        };
        self.expect(from, Observed::Gone);
        self.expect(
            to,
            match hash {
                Some(h) => Observed::Content(h),
                None => Observed::Dir,
            },
        );
        if let Err(e) = fsops::rename(self.policy.root(), from, to) {
            self.echo.remove(from);
            self.echo.remove(to);
            return Err(fs_err(e));
        }
        if let Some(p) = self.pending.remove(from) {
            self.pending.insert(to.to_owned(), p);
        }
        if let Some(h) = self.last_written.remove(from) {
            self.last_written.insert(to.to_owned(), h);
        }
        if self.open.remove(from) {
            self.open.insert(to.to_owned());
        }
        if self.dirty_open.remove(from) {
            self.dirty_open.insert(to.to_owned());
        }
        Ok(Renamed {
            trashed,
            noop: false,
            open,
        })
    }

    /// Apply a remote delete: move to the trash, never remove. A file open in
    /// a tab is left alone until [`close`](Self::close).
    pub fn delete(&mut self, path: &str) -> Result<Deleted, DiskError> {
        let safe = self.policy.check_write(path)?;
        if self.open.contains(path) {
            self.deferred_delete.insert(path.to_owned());
            return Ok(Deleted::DeferredOpen);
        }
        self.pending.remove(path);
        self.last_written.remove(path);
        if safe.abs.symlink_metadata().is_err() {
            return Ok(Deleted::AlreadyGone);
        }
        self.expect(path, Observed::Gone);
        match fsops::trash(self.policy.root(), path) {
            Ok(t) => Ok(Deleted::Trashed(t)),
            Err(e) => {
                self.echo.remove(path);
                Err(fs_err(e))
            }
        }
    }

    /// Join resolution "take the host's version": keep the guest's differing
    /// copy in the trash first, then write the host's bytes now.
    pub fn take_host(&mut self, path: &str, bytes: &[u8]) -> Result<Option<String>, DiskError> {
        let kind = if std::str::from_utf8(bytes).is_ok() {
            NodeKind::Text
        } else {
            NodeKind::Blob
        };
        self.policy.check_size(kind, bytes.len() as u64)?;
        let safe = self.policy.check_write(path)?;
        let backup = match fs::read(&safe.abs) {
            Ok(have) if sha256(&have) != sha256(bytes) => {
                fsops::backup_file(self.policy.root(), path).map_err(fs_err)?
            }
            _ => None,
        };
        self.pending.remove(path);
        self.write_now(path, bytes)?;
        Ok(backup)
    }

    fn expect(&mut self, path: &str, state: Observed) {
        let until_ms = self.clock.now_ms() + ECHO_TTL_MS;
        self.echo.insert(path.to_owned(), Echo { state, until_ms });
    }

    /// Whether the watcher's observation of `path` is the echo of something
    /// this bridge did (and so must not become an intent).
    #[must_use]
    pub fn is_echo(&self, path: &str, observed: Observed) -> bool {
        let now = self.clock.now_ms();
        self.echo
            .get(path)
            .is_some_and(|e| e.until_ms > now && e.state == observed)
    }

    /// Read a file for the tree bridge: `Ok(None)` if absent, a directory
    /// reports `Observed::Dir` via [`observe`](Self::observe).
    pub fn observe(&self, path: &str) -> Result<Observed, DiskError> {
        observe_in(
            self.policy.root().path(),
            path,
            self.policy.max_text().max(crate::pathsec::MAX_BLOB_BYTES),
        )
    }
}

/// What is at `root/rel` now: gone, a directory, or a file with its hash.
/// Symlinks count as gone (they are never shared). Files over `cap` bytes are
/// an error.
pub fn observe_in(root: &Path, rel: &str, cap: u64) -> Result<Observed, DiskError> {
    let abs = root.join(rel);
    let meta = match abs.symlink_metadata() {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Observed::Gone),
        Err(e) => return Err(fs_err(e)),
    };
    if meta.file_type().is_symlink() {
        return Ok(Observed::Gone);
    }
    if meta.is_dir() {
        return Ok(Observed::Dir);
    }
    let bytes = read_capped(&abs, cap)?;
    Ok(Observed::Content(sha256(&bytes)))
}

/// Read a file, refusing more than `cap` bytes.
pub fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, DiskError> {
    let file = fs::File::open(path).map_err(fs_err)?;
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes).map_err(fs_err)?;
    if bytes.len() as u64 > cap {
        return Err(DiskError::Path(PathError::TooLarge {
            size: bytes.len() as u64,
            limit: cap,
        }));
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Join classification
// ---------------------------------------------------------------------------

/// How a local file compares with the host's manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanClass {
    /// Same content hash on both sides.
    Identical,
    /// The host has it and the guest does not.
    Missing,
    /// Both have it with different content (or different kinds).
    Differing,
    /// Only the guest has it (kept private unless shared).
    GuestOnly,
}

impl PlanClass {
    /// The event spelling (`identical`, `missing`, `differing`, `guest-only`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::Missing => "missing",
            Self::Differing => "differing",
            Self::GuestOnly => "guest-only",
        }
    }
}

/// One classified file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanFile {
    /// Project-relative path (the host's spelling for host files).
    pub path: String,
    /// Classification.
    pub class: PlanClass,
}

/// A manifest entry the policy refused (it will never be written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    /// The host's path.
    pub path: String,
    /// Why.
    pub reason: PathError,
}

/// The data of the `live.plan` event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JoinPlan {
    /// Every classified file, sorted by path.
    pub files: Vec<PlanFile>,
    /// Host entries that the security policy refuses.
    pub rejected: Vec<Rejected>,
}

impl JoinPlan {
    /// Files of one class.
    pub fn of(&self, class: PlanClass) -> impl Iterator<Item = &PlanFile> {
        self.files.iter().filter(move |f| f.class == class)
    }

    /// Whether taking the host's version everywhere replaces more than one
    /// local file, which needs the single all-files confirmation.
    #[must_use]
    pub fn needs_all_files_confirmation(&self) -> bool {
        self.of(PlanClass::Differing).count() > 1
    }

    /// The payload of the `live.plan` event: `{files: [{path, class}]}`.
    #[must_use]
    pub fn to_event(&self) -> serde_json::Value {
        json!({
            "files": self.files.iter().map(|f| json!({"path": f.path, "class": f.class.as_str()})).collect::<Vec<_>>(),
        })
    }
}

fn key(policy: &PathPolicy, path: &str) -> String {
    if policy.case_insensitive() {
        path.to_lowercase()
    } else {
        path.to_owned()
    }
}

/// Classify the local folder against the host's manifest.
///
/// * a manifest file whose local counterpart has the same SHA-256 is
///   `identical`, a different one (or a directory in its place) `differing`,
///   none `missing`;
/// * a local shareable file the manifest does not list is `guest-only`;
/// * directories are not classified (they are created as needed); files the
///   security policy refuses, symlinks and unreadable or oversized local
///   files that the host does not list are skipped.
///
/// Never writes anything.
pub fn classify_local(
    policy: &PathPolicy,
    manifest: &[ManifestEntry],
) -> Result<JoinPlan, DiskError> {
    let mut plan = JoinPlan::default();
    let mut listed: BTreeSet<String> = BTreeSet::new();
    let cap = policy.max_text().max(crate::pathsec::MAX_BLOB_BYTES);
    for entry in manifest {
        if let Err(reason) = policy.check_rel(&entry.path) {
            plan.rejected.push(Rejected {
                path: entry.path.clone(),
                reason,
            });
            continue;
        }
        listed.insert(key(policy, &entry.path));
        if entry.kind == NodeKind::Dir {
            continue;
        }
        let class = match observe_in(policy.root().path(), &entry.path, cap) {
            Ok(Observed::Gone) => PlanClass::Missing,
            Ok(Observed::Content(h)) if h == entry.sha256 => PlanClass::Identical,
            // Different content, a directory in the way, or too big to be
            // the same file.
            Ok(Observed::Content(_) | Observed::Dir) | Err(_) => PlanClass::Differing,
        };
        plan.files.push(PlanFile {
            path: entry.path.clone(),
            class,
        });
    }
    let mut guest_only = Vec::new();
    scan_local(policy, "", &listed, &mut guest_only, &mut 0)?;
    for path in guest_only {
        plan.files.push(PlanFile {
            path,
            class: PlanClass::GuestOnly,
        });
    }
    plan.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(plan)
}

fn scan_local(
    policy: &PathPolicy,
    rel_dir: &str,
    listed: &BTreeSet<String>,
    out: &mut Vec<String>,
    seen: &mut usize,
) -> Result<(), DiskError> {
    let dir = policy.root().path().join(rel_dir);
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let rel = if rel_dir.is_empty() {
            name
        } else {
            format!("{rel_dir}/{name}")
        };
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() || policy.check_rel(&rel).is_err() {
            continue;
        }
        *seen += 1;
        if *seen > MAX_SCAN_FILES {
            return Err(DiskError::TooMany);
        }
        if ft.is_dir() {
            scan_local(policy, &rel, listed, out, seen)?;
        } else if ft.is_file() && !listed.contains(&key(policy, &rel)) {
            let size = entry.metadata().map_or(u64::MAX, |m| m.len());
            if size <= crate::pathsec::MAX_BLOB_BYTES {
                out.push(rel);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use manang_live::project::FileId;

    struct Fx {
        tmp: tempfile::TempDir,
        clock: ManualClock,
        disk: DiskBridge<ManualClock>,
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().expect("tmp");
        let clock = ManualClock::new();
        let policy = PathPolicy::new(tmp.path())
            .expect("policy")
            .with_case_insensitive(true);
        let disk = DiskBridge::new(policy, clock.clone());
        Fx { tmp, clock, disk }
    }

    impl Fx {
        fn path(&self, rel: &str) -> std::path::PathBuf {
            self.disk.policy().root().path().join(rel)
        }
        fn read(&self, rel: &str) -> String {
            fs::read_to_string(self.path(rel)).expect("read")
        }
    }

    #[test]
    fn debounced_writes_wait_for_a_quiet_period() {
        let mut f = fx();
        assert_eq!(
            f.disk.queue_text("a.typ", "one").expect("q"),
            WriteOutcome::Queued { due_ms: 500 }
        );
        f.clock.advance(400);
        assert!(f.disk.tick().written.is_empty());
        assert!(!f.path("a.typ").exists());
        // A newer text restarts the quiet period; only the last is written.
        f.disk.queue_text("a.typ", "two").expect("q");
        f.clock.advance(400);
        assert!(f.disk.tick().written.is_empty());
        f.clock.advance(100);
        let report = f.disk.tick();
        assert_eq!(report.written.len(), 1);
        assert_eq!(f.read("a.typ"), "two");
        assert!(report.failed.is_empty());
    }

    #[test]
    fn open_files_are_not_written() {
        let mut f = fx();
        fs::write(f.path("a.typ"), "disk").expect("w");
        f.disk.set_open("a.typ");
        assert_eq!(
            f.disk.queue_text("a.typ", "remote").expect("q"),
            WriteOutcome::DirtyOpen
        );
        assert_eq!(
            f.disk.write_blob("a.typ", b"x").expect("b"),
            WriteOutcome::DirtyOpen
        );
        f.clock.advance(5_000);
        assert!(f.disk.tick().written.is_empty());
        assert_eq!(f.read("a.typ"), "disk");
        assert!(f.disk.is_behind("a.typ"));
        let closed = f.disk.close("a.typ").expect("close");
        assert!(closed.flush_needed);
        assert!(!f.disk.is_behind("a.typ"));
        assert!(!f.disk.close("a.typ").expect("again").flush_needed);
    }

    #[test]
    fn echo_suppression_covers_writes_renames_and_deletes() {
        let mut f = fx();
        f.disk.queue_text("a.typ", "hello").expect("q");
        f.clock.advance(500);
        let done = f.disk.tick();
        let h = done.written[0].sha256;
        assert!(f.disk.is_echo("a.typ", Observed::Content(h)));
        assert!(!f.disk.is_echo("a.typ", Observed::Content(sha256(b"other"))));
        assert!(!f.disk.is_echo("a.typ", Observed::Gone));
        f.disk.rename("a.typ", "b.typ").expect("rename");
        assert!(f.disk.is_echo("a.typ", Observed::Gone));
        assert!(f.disk.is_echo("b.typ", Observed::Content(h)));
        f.disk.delete("b.typ").expect("delete");
        assert!(f.disk.is_echo("b.typ", Observed::Gone));
        f.disk.create_dir("d/e").expect("dir");
        assert!(f.disk.is_echo("d/e", Observed::Dir));
        // Records expire.
        f.clock.advance(ECHO_TTL_MS + 1);
        assert!(!f.disk.is_echo("b.typ", Observed::Gone));
        f.disk.tick();
        assert!(f.disk.echo.is_empty());
    }

    #[test]
    fn delete_goes_to_the_trash_and_is_recoverable() {
        let mut f = fx();
        fs::create_dir_all(f.path("dir")).expect("dir");
        fs::write(f.path("dir/x.md"), "precious").expect("w");
        let Deleted::Trashed(t) = f.disk.delete("dir/x.md").expect("del") else {
            panic!("expected trash");
        };
        assert!(!f.path("dir/x.md").exists());
        assert!(t.starts_with(".manang/trash/"));
        assert!(t.ends_with("/dir/x.md"));
        assert_eq!(fs::read_to_string(f.path(&t)).expect("trash"), "precious");
        assert_eq!(
            f.disk.delete("dir/x.md").expect("again"),
            Deleted::AlreadyGone
        );
    }

    #[test]
    fn deleting_an_open_file_waits_for_the_tab_to_close() {
        let mut f = fx();
        fs::write(f.path("a.md"), "mine").expect("w");
        f.disk.set_open("a.md");
        assert_eq!(f.disk.delete("a.md").expect("d"), Deleted::DeferredOpen);
        assert!(f.path("a.md").exists());
        let closed = f.disk.close("a.md").expect("close");
        let t = closed.trashed.expect("trashed");
        assert!(!f.path("a.md").exists());
        assert_eq!(fs::read_to_string(f.path(&t)).expect("t"), "mine");
        assert!(!closed.flush_needed);
    }

    #[test]
    fn rename_moves_replaces_to_trash_and_follows_pending() {
        let mut f = fx();
        fs::write(f.path("a.md"), "A").expect("w");
        fs::write(f.path("b.md"), "B").expect("w");
        f.disk.queue_text("a.md", "A2").expect("q");
        let r = f.disk.rename("a.md", "sub/b.md").expect("rename");
        assert_eq!(r.trashed, None);
        assert_eq!(f.read("sub/b.md"), "A");
        // The queued write followed the file.
        f.clock.advance(500);
        f.disk.tick();
        assert_eq!(f.read("sub/b.md"), "A2");
        assert!(!f.path("a.md").exists());
        // Replacing an existing destination trashes it first.
        let r = f.disk.rename("sub/b.md", "b.md").expect("rename");
        let t = r.trashed.expect("trashed");
        assert_eq!(fs::read_to_string(f.path(&t)).expect("t"), "B");
        assert_eq!(f.read("b.md"), "A2");
        // Gone source with present destination is a no-op; otherwise an error.
        assert!(f.disk.rename("zzz.md", "b.md").expect("noop").noop);
        assert!(matches!(
            f.disk.rename("zzz.md", "new.md"),
            Err(DiskError::NotFound(_))
        ));
    }

    #[test]
    fn hostile_paths_never_touch_the_disk() {
        let mut f = fx();
        for bad in [
            "../evil.txt",
            "/tmp/evil.txt",
            ".git/hooks/pre-commit",
            "manang.toml",
            ".manang/x",
            "CON",
            "a/b:c",
            "node_modules/x.js",
            ".env",
        ] {
            assert!(
                matches!(f.disk.queue_text(bad, "x"), Err(DiskError::Path(_))),
                "{bad}"
            );
            assert!(f.disk.write_blob(bad, b"x").is_err(), "{bad}");
            assert!(f.disk.delete(bad).is_err(), "{bad}");
            assert!(f.disk.rename(bad, "ok.txt").is_err(), "{bad}");
            assert!(f.disk.rename("ok.txt", bad).is_err(), "{bad}");
            assert!(f.disk.create_dir(bad).is_err(), "{bad}");
        }
        assert!(f.disk.pending.is_empty());
        let names: Vec<_> = fs::read_dir(f.tmp.path()).expect("ls").flatten().collect();
        assert!(names.is_empty(), "{names:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_outside_is_refused_at_queue_and_at_flush() {
        use std::os::unix::fs::symlink;
        let mut f = fx();
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir_all(f.path("docs")).expect("d");
        f.disk.queue_text("docs/x.md", "x").expect("queue ok");
        // The attacker swaps `docs` for a link between queue and flush.
        fs::remove_dir_all(f.path("docs")).expect("rm");
        symlink(outside.path(), f.path("docs")).expect("link");
        f.clock.advance(500);
        let report = f.disk.tick();
        assert!(report.written.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert!(fs::read_dir(outside.path()).expect("ls").next().is_none());
        assert!(f.disk.queue_text("docs/y.md", "y").is_err());
    }

    #[test]
    fn case_collisions_are_refused() {
        let mut f = fx();
        fs::write(f.path("Notes.md"), "n").expect("w");
        assert!(matches!(
            f.disk.queue_text("notes.md", "x"),
            Err(DiskError::Path(PathError::CaseCollision(_)))
        ));
    }

    #[test]
    fn size_caps_are_enforced() {
        let tmp = tempfile::tempdir().expect("tmp");
        let policy = PathPolicy::new(tmp.path()).expect("p").with_caps(4, 8);
        let mut d = DiskBridge::new(policy, ManualClock::new());
        assert!(d.queue_text("a.txt", "12345").is_err());
        assert!(d.write_blob("a.bin", &[0; 9]).is_err());
        assert!(d.write_blob("a.bin", &[0; 8]).is_ok());
    }

    #[test]
    fn a_racing_local_edit_is_backed_up_before_overwriting() {
        let mut f = fx();
        f.disk.queue_text("a.md", "v1").expect("q");
        f.clock.advance(500);
        f.disk.tick();
        fs::write(f.path("a.md"), "local edit").expect("edit");
        f.disk.queue_text("a.md", "v2").expect("q");
        f.clock.advance(500);
        let report = f.disk.tick();
        let backup = report.written[0].backup.clone().expect("backup");
        assert_eq!(
            fs::read_to_string(f.path(&backup)).expect("b"),
            "local edit"
        );
        assert_eq!(f.read("a.md"), "v2");
        // No backup when the disk still holds what we wrote.
        f.disk.queue_text("a.md", "v3").expect("q");
        f.clock.advance(500);
        assert!(f.disk.tick().written[0].backup.is_none());
    }

    #[test]
    fn unchanged_content_is_not_rewritten() {
        let mut f = fx();
        fs::write(f.path("a.md"), "same").expect("w");
        f.disk.queue_text("a.md", "same").expect("q");
        f.clock.advance(500);
        assert!(f.disk.tick().written.is_empty());
        assert!(!f.disk.is_echo("a.md", Observed::Content(sha256(b"same"))));
    }

    #[test]
    fn take_host_keeps_the_guests_copy() {
        let mut f = fx();
        fs::write(f.path("a.md"), "guest").expect("w");
        let backup = f.disk.take_host("a.md", b"host").expect("take").expect("b");
        assert_eq!(f.read("a.md"), "host");
        assert_eq!(fs::read_to_string(f.path(&backup)).expect("b"), "guest");
        assert_eq!(f.disk.take_host("a.md", b"host").expect("again"), None);
    }

    fn entry(path: &str, content: &[u8], kind: NodeKind) -> ManifestEntry {
        ManifestEntry {
            id: FileId(1),
            path: path.to_owned(),
            kind,
            size: content.len() as u64,
            sha256: sha256(content),
            seed_fp: [0; 16],
        }
    }

    #[test]
    fn join_plan_classifies_identical_missing_differing_guest_only() {
        let mut f = fx();
        fs::create_dir_all(f.path("src")).expect("d");
        fs::write(f.path("same.typ"), "same").expect("w");
        fs::write(f.path("src/diff.typ"), "mine").expect("w");
        fs::write(f.path("private.md"), "p").expect("w");
        fs::write(f.path("src/deep-private.md"), "p").expect("w");
        fs::write(f.path(".env"), "secret").expect("w");
        fs::write(f.path("manang.toml"), "x").expect("w");
        fs::create_dir_all(f.path("target")).expect("d");
        fs::write(f.path("target/out.pdf"), "x").expect("w");
        fs::write(f.path(".gitignore"), "*.log\n").expect("w");
        fs::write(f.path("run.log"), "x").expect("w");
        f.disk.policy_mut().reload_ignore();
        let manifest = vec![
            entry("src", b"", NodeKind::Dir),
            entry("same.typ", b"same", NodeKind::Text),
            entry("src/diff.typ", b"theirs", NodeKind::Text),
            entry("new.typ", b"new", NodeKind::Text),
            entry("fig.png", &[1, 2, 3], NodeKind::Blob),
            entry(".git/config", b"x", NodeKind::Blob),
            entry("../up.typ", b"x", NodeKind::Text),
        ];
        let plan = classify_local(f.disk.policy(), &manifest).expect("plan");
        let got: Vec<(&str, PlanClass)> = plan
            .files
            .iter()
            .map(|p| (p.path.as_str(), p.class))
            .collect();
        assert_eq!(
            got,
            vec![
                ("fig.png", PlanClass::Missing),
                ("new.typ", PlanClass::Missing),
                ("private.md", PlanClass::GuestOnly),
                ("same.typ", PlanClass::Identical),
                ("src/deep-private.md", PlanClass::GuestOnly),
                ("src/diff.typ", PlanClass::Differing),
            ]
        );
        assert_eq!(plan.rejected.len(), 2);
        assert!(!plan.needs_all_files_confirmation());
        assert_eq!(
            plan.to_event()["files"][0],
            json!({"path": "fig.png", "class": "missing"})
        );
        assert_eq!(plan.to_event()["files"][2]["class"], "guest-only");
        // Nothing was written.
        assert_eq!(f.read("src/diff.typ"), "mine");
    }

    #[test]
    fn join_plan_with_two_differing_files_needs_the_all_files_confirmation() {
        let f = fx();
        fs::write(f.path("a.md"), "1").expect("w");
        fs::write(f.path("b.md"), "2").expect("w");
        let manifest = vec![
            entry("a.md", b"x", NodeKind::Text),
            entry("b.md", b"y", NodeKind::Text),
        ];
        let plan = classify_local(f.disk.policy(), &manifest).expect("plan");
        assert!(plan.needs_all_files_confirmation());
    }

    #[cfg(unix)]
    #[test]
    fn join_plan_ignores_symlinks() {
        use std::os::unix::fs::symlink;
        let f = fx();
        let outside = tempfile::tempdir().expect("o");
        fs::write(outside.path().join("secret.txt"), "s").expect("w");
        symlink(outside.path(), f.path("link")).expect("l");
        symlink(outside.path().join("secret.txt"), f.path("s.txt")).expect("l");
        let plan = classify_local(f.disk.policy(), &[]).expect("plan");
        assert!(plan.files.is_empty());
    }

    #[test]
    fn case_insensitive_plan_matches_names_regardless_of_case() {
        let f = fx();
        fs::write(f.path("Readme.md"), "same").expect("w");
        let plan = classify_local(
            f.disk.policy(),
            &[entry("README.md", b"same", NodeKind::Text)],
        )
        .expect("plan");
        // Listed with the host's spelling; the local file is not guest-only.
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].path, "README.md");
    }
}
