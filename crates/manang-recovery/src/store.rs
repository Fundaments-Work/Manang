//! The on-disk snapshot store.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use manang_formats::{FormatError, set_aside_corrupt, write_atomic};
use manang_project::fsops::{file_hash, hash_bytes};
use manang_project::{ProjectError, ProjectRoot, lexical_relative};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Largest document text kept in a snapshot.
pub const MAX_SNAPSHOT_BYTES: usize = 5 * 1024 * 1024;
/// Largest view-state blob (caret, scroll) kept with a snapshot.
const MAX_VIEW_BYTES: usize = 4 * 1024;
/// The snapshot file format version.
pub const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("invalid path: {0}")]
    Path(#[from] ProjectError),
    #[error(transparent)]
    Format(#[from] FormatError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("could not serialise a snapshot: {0}")]
    Serialise(#[from] serde_json::Error),
}

/// What a snapshot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The text of a document.
    Text,
    /// The scene JSON of a figure.
    Figure,
}

/// A stored snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub kind: Kind,
    /// Absolute project root, for reading by a person.
    pub project: String,
    /// Project-relative path of the document.
    pub path: String,
    /// Fingerprint of the saved text this edit started from; `None` for a
    /// file that did not exist yet.
    pub saved_hash: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub saved_at: u64,
    pub text: String,
    /// Caret, scroll and other tab view state; opaque to Rust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<Value>,
}

/// A request to store a snapshot.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteRequest {
    pub project: String,
    pub path: String,
    pub kind: Kind,
    pub content: String,
    #[serde(default)]
    pub saved_hash: Option<String>,
    #[serde(default)]
    pub view: Option<Value>,
}

/// Whether a snapshot was stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteOutcome {
    pub stored: bool,
    /// Why not: `too-large`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    pub limit: usize,
}

/// How a snapshot's saved text compares with the file on disk now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskState {
    /// The file is still what the edit started from.
    Match,
    /// The file changed since (another editor, git pull...).
    Changed,
    /// The file is gone, or never existed yet.
    Missing,
    /// The project folder could not be inspected.
    Unknown,
}

/// A snapshot without its text, for the recovery dialog.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub kind: Kind,
    pub path: String,
    pub saved_at: u64,
    pub bytes: usize,
    pub disk: DiskState,
}

/// Every usable snapshot of a project.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Unreadable snapshots renamed to `.corrupt` during this listing.
    pub set_aside: usize,
}

fn io_err(context: impl Into<String>, source: io::Error) -> RecoveryError {
    RecoveryError::Io {
        context: context.into(),
        source,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

fn short_hash(text: &str) -> String {
    // "b3:" + 32 hex; 16 hex digits are plenty for a folder name.
    hash_bytes(text.as_bytes())
        .trim_start_matches("b3:")
        .chars()
        .take(16)
        .collect()
}

/// The directory of snapshots, one subfolder per project.
#[derive(Debug)]
pub struct RecoveryStore {
    root: PathBuf,
    /// Serialises writes so two snapshots of one file never interleave.
    lock: Mutex<()>,
}

impl RecoveryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            lock: Mutex::new(()),
        }
    }

    fn project_dir(&self, project: &str) -> PathBuf {
        self.root.join(short_hash(project.trim_end_matches('/')))
    }

    fn file_for(&self, project: &str, rel: &str) -> PathBuf {
        self.project_dir(project)
            .join(format!("{}.json", short_hash(rel)))
    }

    /// Store a snapshot, replacing the previous one of that file.
    pub fn write(&self, req: WriteRequest) -> Result<WriteOutcome, RecoveryError> {
        let rel = lexical_relative(&req.path)?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        if req.content.len() > MAX_SNAPSHOT_BYTES {
            return Ok(WriteOutcome {
                stored: false,
                reason: Some("too-large"),
                limit: MAX_SNAPSHOT_BYTES,
            });
        }
        let view = req.view.filter(|v| {
            serde_json::to_string(v)
                .map(|s| s.len() <= MAX_VIEW_BYTES)
                .unwrap_or(false)
        });
        let snapshot = Snapshot {
            version: SNAPSHOT_VERSION,
            kind: req.kind,
            project: req.project.trim_end_matches('/').to_owned(),
            path: rel.clone(),
            saved_hash: req.saved_hash,
            saved_at: now_ms(),
            text: req.content,
            view,
        };
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        let file = self.file_for(&snapshot.project, &rel);
        let _guard = self.lock.lock();
        write_atomic(&file, &bytes)?;
        Ok(WriteOutcome {
            stored: true,
            reason: None,
            limit: MAX_SNAPSHOT_BYTES,
        })
    }

    /// Read a snapshot file. `Ok(None)`: absent, from a newer Manang, or
    /// unreadable (renamed aside, counted in `aside`).
    fn load(&self, file: &Path, aside: &mut usize) -> Option<Snapshot> {
        let text = match fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => {
                log::warn!("recovery: cannot read {}: {e}", file.display());
                self.set_aside(file, aside);
                return None;
            }
        };
        let parsed: Result<Snapshot, _> = serde_json::from_str(&text);
        match parsed {
            Ok(s) if s.version > SNAPSHOT_VERSION => {
                log::warn!(
                    "recovery: {} was written by a newer Manang (version {}); leaving it alone",
                    file.display(),
                    s.version
                );
                None
            }
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!(
                    "recovery: {} is corrupt ({e}); setting it aside",
                    file.display()
                );
                self.set_aside(file, aside);
                None
            }
        }
    }

    fn set_aside(&self, file: &Path, aside: &mut usize) {
        match set_aside_corrupt(file) {
            Ok(to) => {
                *aside += 1;
                log::warn!("recovery: kept the unreadable snapshot as {}", to.display());
            }
            Err(e) => log::warn!("recovery: could not set {} aside: {e}", file.display()),
        }
    }

    /// Every usable snapshot of `project`, newest first, with how each one's
    /// file compares with the disk now.
    pub fn list(&self, project: &str) -> Listing {
        let dir = self.project_dir(project);
        let mut listing = Listing::default();
        let Ok(read) = fs::read_dir(&dir) else {
            return listing;
        };
        let root = ProjectRoot::open(Path::new(project.trim_end_matches('/'))).ok();
        let _guard = self.lock.lock();
        for item in read.flatten() {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(snapshot) = self.load(&path, &mut listing.set_aside) else {
                continue;
            };
            let disk = disk_state(root.as_ref(), &snapshot);
            listing.entries.push(Entry {
                kind: snapshot.kind,
                path: snapshot.path,
                saved_at: snapshot.saved_at,
                bytes: snapshot.text.len(),
                disk,
            });
        }
        listing
            .entries
            .sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then(a.path.cmp(&b.path)));
        listing
    }

    /// One snapshot, text included.
    pub fn read(&self, project: &str, path: &str) -> Result<Option<Snapshot>, RecoveryError> {
        let rel = lexical_relative(path)?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let file = self.file_for(project, &rel);
        let _guard = self.lock.lock();
        let mut aside = 0;
        Ok(self.load(&file, &mut aside))
    }

    /// Remove one snapshot (`path`) or all of a project's (`None`). Returns how
    /// many snapshot files were removed.
    pub fn discard(&self, project: &str, path: Option<&str>) -> Result<usize, RecoveryError> {
        let _guard = self.lock.lock();
        match path {
            Some(path) => {
                let rel = lexical_relative(path)?;
                let rel = rel.to_string_lossy().replace('\\', "/");
                remove_file(&self.file_for(project, &rel))
            }
            None => {
                let dir = self.project_dir(project);
                let Ok(read) = fs::read_dir(&dir) else {
                    return Ok(0);
                };
                let mut removed = 0;
                for item in read.flatten() {
                    let p = item.path();
                    if p.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    // A newer Manang's snapshot is not ours to delete.
                    if is_newer(&p) {
                        continue;
                    }
                    removed += remove_file(&p)?;
                }
                // Leave the folder when set-aside files remain; drop it when empty.
                let _ = fs::remove_dir(&dir);
                Ok(removed)
            }
        }
    }
}

/// Whether a snapshot file says it is from a newer format than this build's.
fn is_newer(file: &Path) -> bool {
    #[derive(Deserialize)]
    struct Head {
        version: u32,
    }
    fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<Head>(&t).ok())
        .is_some_and(|h| h.version > SNAPSHOT_VERSION)
}

fn remove_file(file: &Path) -> Result<usize, RecoveryError> {
    match fs::remove_file(file) {
        Ok(()) => Ok(1),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(io_err(format!("removing {}", file.display()), e)),
    }
}

fn disk_state(root: Option<&ProjectRoot>, snapshot: &Snapshot) -> DiskState {
    let Some(root) = root else {
        return DiskState::Unknown;
    };
    match file_hash(root, &snapshot.path) {
        Ok(hash) => match &snapshot.saved_hash {
            Some(saved) if *saved == hash => DiskState::Match,
            // A figure's scene is saved by the figure plugin, which has no
            // fingerprint to give: say nothing rather than guess.
            None if snapshot.kind == Kind::Figure => DiskState::Unknown,
            _ => DiskState::Changed,
        },
        Err(ProjectError::NotFound(_)) => DiskState::Missing,
        Err(_) => DiskState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(project: &str, path: &str, content: &str, hash: Option<&str>) -> WriteRequest {
        WriteRequest {
            project: project.to_owned(),
            path: path.to_owned(),
            kind: Kind::Text,
            content: content.to_owned(),
            saved_hash: hash.map(str::to_owned),
            view: None,
        }
    }

    struct Harness {
        _tmp: tempfile::TempDir,
        store: RecoveryStore,
        project: PathBuf,
    }

    fn harness() -> Harness {
        let tmp = tempfile::tempdir().expect("tmp");
        let project = tmp.path().join("proj");
        fs::create_dir_all(&project).expect("project");
        let store = RecoveryStore::new(tmp.path().join("recovery"));
        Harness {
            _tmp: tmp,
            store,
            project,
        }
    }

    impl Harness {
        fn p(&self) -> String {
            self.project.to_string_lossy().into_owned()
        }
    }

    #[test]
    fn a_snapshot_round_trips_and_is_replaced_by_the_next() {
        let h = harness();
        let out = h
            .store
            .write(request(&h.p(), "main.typ", "one", None))
            .expect("write");
        assert!(out.stored);
        h.store
            .write(request(&h.p(), "main.typ", "two", None))
            .expect("write");
        let got = h
            .store
            .read(&h.p(), "main.typ")
            .expect("read")
            .expect("some");
        assert_eq!(got.text, "two");
        assert_eq!(got.path, "main.typ");
        assert_eq!(h.store.list(&h.p()).entries.len(), 1);
    }

    #[test]
    fn oversized_text_is_refused_without_touching_the_old_snapshot() {
        let h = harness();
        h.store
            .write(request(&h.p(), "a.typ", "small", None))
            .expect("write");
        let big = "x".repeat(MAX_SNAPSHOT_BYTES + 1);
        let out = h
            .store
            .write(request(&h.p(), "a.typ", &big, None))
            .expect("write");
        assert!(!out.stored);
        assert_eq!(out.reason, Some("too-large"));
        let kept = h.store.read(&h.p(), "a.typ").expect("read").expect("some");
        assert_eq!(kept.text, "small");
    }

    #[test]
    fn paths_that_escape_the_project_are_refused() {
        let h = harness();
        assert!(h.store.write(request(&h.p(), "../x", "t", None)).is_err());
        assert!(h.store.write(request(&h.p(), "/etc/x", "t", None)).is_err());
        assert!(h.store.discard(&h.p(), Some("../x")).is_err());
    }

    #[test]
    fn the_disk_state_follows_the_saved_hash() {
        let h = harness();
        fs::write(h.project.join("a.typ"), "saved").expect("file");
        let hash = hash_bytes(b"saved");
        h.store
            .write(request(&h.p(), "a.typ", "edited", Some(&hash)))
            .expect("write");
        h.store
            .write(request(&h.p(), "gone.typ", "x", Some(&hash)))
            .expect("write");
        let states = |h: &Harness| {
            h.store
                .list(&h.p())
                .entries
                .into_iter()
                .map(|e| (e.path, e.disk))
                .collect::<Vec<_>>()
        };
        let mut now = states(&h);
        now.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            now,
            vec![
                ("a.typ".to_owned(), DiskState::Match),
                ("gone.typ".to_owned(), DiskState::Missing)
            ]
        );
        fs::write(h.project.join("a.typ"), "changed elsewhere").expect("file");
        assert!(states(&h).contains(&("a.typ".to_owned(), DiskState::Changed)));
    }

    #[test]
    fn discard_removes_one_or_all() {
        let h = harness();
        h.store.write(request(&h.p(), "a", "1", None)).expect("w");
        h.store.write(request(&h.p(), "b", "2", None)).expect("w");
        assert_eq!(h.store.discard(&h.p(), Some("a")).expect("d"), 1);
        assert_eq!(h.store.discard(&h.p(), Some("a")).expect("d"), 0);
        assert_eq!(h.store.list(&h.p()).entries.len(), 1);
        assert_eq!(h.store.discard(&h.p(), None).expect("d"), 1);
        assert!(h.store.list(&h.p()).entries.is_empty());
    }

    #[test]
    fn projects_do_not_see_each_others_snapshots() {
        let h = harness();
        h.store.write(request("/one", "a", "1", None)).expect("w");
        h.store.write(request("/two", "a", "2", None)).expect("w");
        assert_eq!(h.store.list("/one").entries.len(), 1);
        assert_eq!(h.store.discard("/one", None).expect("d"), 1);
        assert_eq!(h.store.list("/two").entries.len(), 1);
    }

    #[test]
    fn a_corrupt_snapshot_is_set_aside_and_the_rest_still_list() {
        let h = harness();
        h.store
            .write(request(&h.p(), "good", "ok", None))
            .expect("w");
        h.store
            .write(request(&h.p(), "bad", "ok", None))
            .expect("w");
        let bad = h.store.file_for(&h.p(), "bad");
        fs::write(&bad, "{ not json").expect("corrupt it");
        let listing = h.store.list(&h.p());
        assert_eq!(listing.set_aside, 1);
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path, "good");
        assert!(!bad.exists());
        let aside = bad.with_file_name(format!(
            "{}.corrupt",
            bad.file_name().and_then(|n| n.to_str()).unwrap_or_default()
        ));
        assert!(aside.exists(), "the broken file is kept, not deleted");
        // A second listing is quiet.
        assert_eq!(h.store.list(&h.p()).set_aside, 0);
    }

    #[test]
    fn a_newer_snapshot_is_left_alone_and_not_offered() {
        let h = harness();
        h.store.write(request(&h.p(), "n", "ok", None)).expect("w");
        let file = h.store.file_for(&h.p(), "n");
        let text = fs::read_to_string(&file).expect("read");
        let newer = text.replace("\"version\": 1", "\"version\": 999");
        fs::write(&file, &newer).expect("write");
        let listing = h.store.list(&h.p());
        assert!(listing.entries.is_empty());
        assert_eq!(listing.set_aside, 0);
        assert_eq!(fs::read_to_string(&file).expect("read"), newer);
    }

    #[test]
    fn concurrent_writers_never_leave_a_torn_file() {
        let h = harness();
        let store = std::sync::Arc::new(h.store);
        let project = h.project.to_string_lossy().into_owned();
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let store = std::sync::Arc::clone(&store);
                let project = project.clone();
                std::thread::spawn(move || {
                    for i in 0..25 {
                        let text = format!("writer {t} edit {i} {}", "y".repeat(2000));
                        store
                            .write(request(&project, "shared.typ", &text, None))
                            .expect("write");
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("join");
        }
        let listing = store.list(&project);
        assert_eq!(listing.set_aside, 0);
        assert_eq!(listing.entries.len(), 1);
        let got = store.read(&project, "shared.typ").expect("r").expect("s");
        assert!(got.text.starts_with("writer "));
        assert!(got.text.len() > 2000);
    }

    #[test]
    fn view_state_is_kept_when_small_and_dropped_when_huge() {
        let h = harness();
        let mut req = request(&h.p(), "v", "t", None);
        req.view = Some(serde_json::json!({ "anchor": 3, "head": 3, "scroll": 120 }));
        h.store.write(req).expect("w");
        let got = h.store.read(&h.p(), "v").expect("r").expect("s");
        assert_eq!(
            got.view,
            Some(serde_json::json!({ "anchor": 3, "head": 3, "scroll": 120 }))
        );
        let mut req = request(&h.p(), "v", "t", None);
        req.view = Some(Value::String("z".repeat(MAX_VIEW_BYTES + 10)));
        h.store.write(req).expect("w");
        assert!(
            h.store
                .read(&h.p(), "v")
                .expect("r")
                .expect("s")
                .view
                .is_none()
        );
    }
}
