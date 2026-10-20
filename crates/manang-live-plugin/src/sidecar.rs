//! Disposable per-room, per-file document snapshots (the "sidecar").
//!
//! A project session keeps one Loro document per text file, loaded lazily. To
//! stay inside the memory budget the session evicts the least recently used
//! documents ([`Residency`]) and parks each as a **full** snapshot (never
//! trimmed: a shallow snapshot would lose history that peers may still need)
//! in `<data>/manang-live/<room>/<fileid>.loro` ([`SidecarStore`]). Touching
//! the file again rehydrates it from there instead of re-seeding from disk.
//!
//! The sidecar is a cache, never a source of truth:
//!
//! * every sidecar records the SHA-256 of the file's bytes on disk at the time
//!   it was written; [`SidecarStore::load`] hands the bytes back only when the
//!   disk still has exactly that content, so a file edited outside the
//!   session (or by a crash between sidecar and disk write) is re-seeded from
//!   disk and the stale sidecar is deleted;
//! * the body carries its own SHA-256 and length; any damage (truncation,
//!   flipped bits, foreign file, bad header) makes the sidecar a
//!   [`Rehydrated::Discarded`] miss, never an error;
//! * if the caller cannot import the bytes into Loro it calls
//!   [`SidecarStore::discard`];
//! * deleting the whole directory loses nothing the room or the disk lacks.
//!
//! Names are validated (room id: lowercase hex; file id: [`FileId`]) so a
//! name can never leave the directory. Pruning ([`SidecarStore::prune`],
//! [`SidecarStore::retain`], [`SidecarStore::enforce_budget`]) takes the
//! current time as a parameter so it is testable.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use manang_live::blob::blob_hash;
use manang_live::project::FileId;
use manang_project::fsops;
use thiserror::Error;

const MAGIC: &[u8; 4] = b"MLF1";
const HEADER: usize = 4 + 32 + 32 + 8;
/// Largest sidecar read back (a guard against a damaged or foreign file).
pub const MAX_SIDECAR_BYTES: u64 = 256 * 1024 * 1024;
/// Sidecars untouched for this long are pruned by default.
pub const DEFAULT_KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Default cap on documents held in memory at once.
pub const DEFAULT_LOADED_DOCS: usize = 32;

/// Sidecar failures (a damaged sidecar is not one of them).
#[derive(Debug, Error)]
pub enum SidecarError {
    /// A room or file name that is not hex or a file id.
    #[error("not a valid sidecar name")]
    BadName,
    /// The file system refused.
    #[error("sidecar i/o: {0}")]
    Io(String),
}

/// What [`SidecarStore::load`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rehydrated {
    /// A full snapshot, consistent with the disk.
    Hit(Vec<u8>),
    /// No sidecar.
    Miss,
    /// A sidecar existed but was damaged or stale; it has been deleted.
    Discarded(&'static str),
}

/// The sidecar directory, `<data>/manang-live`.
#[derive(Debug, Clone)]
pub struct SidecarStore {
    root: PathBuf,
}

impl SidecarStore {
    /// A store under the app data directory `data_dir`.
    #[must_use]
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("manang-live"),
        }
    }

    fn room_dir(&self, room_hex: &str) -> Result<PathBuf, SidecarError> {
        let ok = !room_hex.is_empty()
            && room_hex.len() <= 64
            && room_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        ok.then(|| self.root.join(room_hex))
            .ok_or(SidecarError::BadName)
    }

    fn file(&self, room_hex: &str, id: FileId) -> Result<PathBuf, SidecarError> {
        Ok(self
            .room_dir(room_hex)?
            .join(format!("{}.loro", id.to_hex())))
    }

    /// Park a full snapshot of file `id` whose bytes on disk hash to
    /// `disk_sha`, replacing the previous one atomically.
    pub fn save(
        &self,
        room_hex: &str,
        id: FileId,
        disk_sha: &[u8; 32],
        snapshot: &[u8],
    ) -> Result<(), SidecarError> {
        let path = self.file(room_hex, id)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| SidecarError::Io(e.to_string()))?;
        }
        let mut bytes = Vec::with_capacity(HEADER + snapshot.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(disk_sha);
        bytes.extend_from_slice(&blob_hash(snapshot));
        bytes.extend_from_slice(&(snapshot.len() as u64).to_le_bytes());
        bytes.extend_from_slice(snapshot);
        fsops::atomic_write(&path, &bytes).map_err(|e| SidecarError::Io(e.to_string()))
    }

    /// Rehydrate file `id`, trusting the sidecar only if it is intact and was
    /// written for exactly the content now on disk (`disk_sha`). A bad or
    /// stale sidecar is deleted.
    #[must_use]
    pub fn load(&self, room_hex: &str, id: FileId, disk_sha: &[u8; 32]) -> Rehydrated {
        let Ok(path) = self.file(room_hex, id) else {
            return Rehydrated::Miss;
        };
        let Ok(meta) = fs::metadata(&path) else {
            return Rehydrated::Miss;
        };
        let verdict = if meta.len() > MAX_SIDECAR_BYTES {
            Err("too large")
        } else {
            match fs::read(&path) {
                Ok(bytes) => parse(&bytes, disk_sha),
                Err(_) => Err("unreadable"),
            }
        };
        match verdict {
            Ok(body) => Rehydrated::Hit(body),
            Err(why) => {
                let _ = fs::remove_file(&path);
                Rehydrated::Discarded(why)
            }
        }
    }

    /// Delete one sidecar (the caller could not import it, or the file is gone).
    pub fn discard(&self, room_hex: &str, id: FileId) {
        if let Ok(path) = self.file(room_hex, id) {
            let _ = fs::remove_file(path);
        }
    }

    /// Delete every sidecar of a room.
    pub fn remove_room(&self, room_hex: &str) {
        if let Ok(dir) = self.room_dir(room_hex) {
            let _ = fs::remove_dir_all(dir);
        }
    }

    /// Keep only the sidecars of files in `live` (files deleted from the
    /// project tree). Returns how many were removed.
    pub fn retain(&self, room_hex: &str, live: &BTreeSet<FileId>) -> usize {
        let Ok(dir) = self.room_dir(room_hex) else {
            return 0;
        };
        let mut removed = 0;
        for (path, id, _, _) in entries(&dir) {
            if id.is_none_or(|id| !live.contains(&id)) && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Remove sidecars not modified for `max_age` as of `now`, and room
    /// folders left empty. Returns how many sidecars were removed.
    pub fn prune(&self, max_age: Duration, now: SystemTime) -> usize {
        let mut removed = 0;
        let Ok(rooms) = fs::read_dir(&self.root) else {
            return 0;
        };
        for room in rooms.flatten() {
            let dir = room.path();
            if !dir.is_dir() {
                continue;
            }
            for (path, _, _, modified) in entries(&dir) {
                let old = now.duration_since(modified).is_ok_and(|age| age > max_age);
                if old && fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
            // Fails (and is ignored) unless the folder is empty.
            let _ = fs::remove_dir(&dir);
        }
        removed
    }

    /// Delete the oldest sidecars until all together take at most `max_bytes`.
    /// Returns how many were removed.
    pub fn enforce_budget(&self, max_bytes: u64) -> usize {
        let mut all = Vec::new();
        if let Ok(rooms) = fs::read_dir(&self.root) {
            for room in rooms.flatten() {
                all.extend(entries(&room.path()));
            }
        }
        let mut total: u64 = all.iter().map(|e| e.2).sum();
        all.sort_by_key(|e| e.3);
        let mut removed = 0;
        for e in all {
            if total <= max_bytes {
                break;
            }
            if fs::remove_file(&e.0).is_ok() {
                total = total.saturating_sub(e.2);
                removed += 1;
            }
        }
        removed
    }
}

/// `(path, parsed id, size, modified)` of every `*.loro` in `dir`.
type Entry = (PathBuf, Option<FileId>, u64, SystemTime);

fn entries(dir: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(read) = fs::read_dir(dir) else {
        return out;
    };
    for e in read.flatten() {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "loro") {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(FileId::from_hex);
        out.push((
            path,
            id,
            meta.len(),
            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        ));
    }
    out
}

fn parse(bytes: &[u8], disk_sha: &[u8; 32]) -> Result<Vec<u8>, &'static str> {
    if bytes.len() < HEADER || &bytes[..4] != MAGIC {
        return Err("bad header");
    }
    if bytes[4..36] != disk_sha[..] {
        return Err("disk changed since the snapshot");
    }
    let body_sha = &bytes[36..68];
    let mut len = [0u8; 8];
    len.copy_from_slice(&bytes[68..76]);
    let body = &bytes[HEADER..];
    if u64::from_le_bytes(len) != body.len() as u64 {
        return Err("truncated");
    }
    if blob_hash(body)[..] != *body_sha {
        return Err("checksum mismatch");
    }
    Ok(body.to_vec())
}

/// Least-recently-used bookkeeping for loaded documents.
///
/// The session calls [`touch`](Self::touch) whenever it uses a document and
/// parks every key it returns (save a snapshot, drop the document). Pinned
/// keys (files open in a tab, files with unsaved peers' updates) are never
/// returned, so the loaded count may exceed the cap while many are pinned.
#[derive(Debug, Clone)]
pub struct Residency<K: Ord + Clone> {
    cap: usize,
    /// Least recently used first.
    order: Vec<K>,
    pinned: BTreeSet<K>,
}

impl<K: Ord + Clone> Residency<K> {
    /// Allow at most `cap` loaded documents (at least 1).
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            order: Vec::new(),
            pinned: BTreeSet::new(),
        }
    }

    /// Mark `key` most recently used (loading it if new); returns the keys to
    /// evict to get back under the cap.
    pub fn touch(&mut self, key: K) -> Vec<K> {
        self.order.retain(|k| *k != key);
        self.order.push(key);
        let mut evicted = Vec::new();
        while self.order.len() > self.cap {
            let Some(i) = self.order.iter().position(|k| !self.pinned.contains(k)) else {
                break;
            };
            evicted.push(self.order.remove(i));
        }
        evicted
    }

    /// Never evict `key` until unpinned.
    pub fn pin(&mut self, key: K) {
        self.pinned.insert(key);
    }

    /// Allow evicting `key` again; returns keys now over the cap.
    pub fn unpin(&mut self, key: &K) -> Vec<K> {
        self.pinned.remove(key);
        let mut evicted = Vec::new();
        while self.order.len() > self.cap {
            let Some(i) = self.order.iter().position(|k| !self.pinned.contains(k)) else {
                break;
            };
            evicted.push(self.order.remove(i));
        }
        evicted
    }

    /// Forget a document that is gone.
    pub fn remove(&mut self, key: &K) {
        self.order.retain(|k| k != key);
        self.pinned.remove(key);
    }

    /// Whether `key` is loaded.
    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.order.contains(key)
    }

    /// Loaded documents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether nothing is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: &str = "0a1b2c3d";

    fn store() -> (tempfile::TempDir, SidecarStore) {
        let tmp = tempfile::tempdir().expect("tmp");
        let s = SidecarStore::new(tmp.path());
        (tmp, s)
    }

    fn sha(b: &[u8]) -> [u8; 32] {
        blob_hash(b)
    }

    #[test]
    fn round_trips_a_full_snapshot_when_the_disk_matches() {
        let (tmp, s) = store();
        let id = FileId(7);
        s.save(ROOM, id, &sha(b"disk"), b"snapshot-bytes")
            .expect("save");
        let path = tmp
            .path()
            .join("manang-live")
            .join(ROOM)
            .join(format!("{}.loro", id.to_hex()));
        assert!(path.exists());
        assert_eq!(
            s.load(ROOM, id, &sha(b"disk")),
            Rehydrated::Hit(b"snapshot-bytes".to_vec())
        );
        // Still there after a hit (it is a cache, not a queue).
        assert!(path.exists());
        assert_eq!(s.load(ROOM, FileId(8), &sha(b"disk")), Rehydrated::Miss);
    }

    #[test]
    fn a_sidecar_is_never_trusted_over_the_disk() {
        let (_tmp, s) = store();
        let id = FileId(1);
        s.save(ROOM, id, &sha(b"old disk"), b"snap").expect("save");
        assert!(matches!(
            s.load(ROOM, id, &sha(b"edited outside")),
            Rehydrated::Discarded(_)
        ));
        // Deleted: the next load is a plain miss.
        assert_eq!(s.load(ROOM, id, &sha(b"old disk")), Rehydrated::Miss);
    }

    #[test]
    fn corruption_is_discarded_not_fatal() {
        let (tmp, s) = store();
        let disk = sha(b"d");
        for (n, damage) in [
            |b: &mut Vec<u8>| b.truncate(10),
            |b: &mut Vec<u8>| {
                let n = b.len();
                b.truncate(n - 3);
            },
            |b: &mut Vec<u8>| {
                let n = b.len();
                b[n - 1] ^= 0xff;
            },
            |b: &mut Vec<u8>| b[0] = b'X',
            |b: &mut Vec<u8>| b.clear(),
        ]
        .into_iter()
        .enumerate()
        {
            let id = FileId(n as u128 + 1);
            s.save(ROOM, id, &disk, b"a reasonably long snapshot body")
                .expect("save");
            let path = tmp
                .path()
                .join("manang-live")
                .join(ROOM)
                .join(format!("{}.loro", id.to_hex()));
            let mut bytes = fs::read(&path).expect("read");
            damage(&mut bytes);
            fs::write(&path, bytes).expect("write");
            assert!(
                matches!(s.load(ROOM, id, &disk), Rehydrated::Discarded(_)),
                "case {n}"
            );
            assert!(!path.exists());
        }
    }

    #[test]
    fn names_cannot_leave_the_directory() {
        let (_tmp, s) = store();
        for bad in ["", "../x", "ZZ", "a/b", "0A1B", &"a".repeat(65)] {
            assert!(
                matches!(
                    s.save(bad, FileId(1), &[0; 32], b"x"),
                    Err(SidecarError::BadName)
                ),
                "{bad}"
            );
            assert_eq!(s.load(bad, FileId(1), &[0; 32]), Rehydrated::Miss);
        }
    }

    #[test]
    fn retain_prune_budget_and_remove_room() {
        let (tmp, s) = store();
        let d = sha(b"d");
        for i in 1..=4u128 {
            s.save(ROOM, FileId(i), &d, &[0u8; 100]).expect("save");
        }
        s.save("ff", FileId(1), &d, &[0u8; 100]).expect("save");
        // Files deleted from the tree lose their sidecars.
        let live: BTreeSet<FileId> = [FileId(1), FileId(2)].into();
        assert_eq!(s.retain(ROOM, &live), 2);
        assert_eq!(s.load(ROOM, FileId(3), &d), Rehydrated::Miss);
        assert!(matches!(s.load(ROOM, FileId(1), &d), Rehydrated::Hit(_)));
        // A stray non-sidecar name is left alone, a stray .loro is pruned.
        let dir = tmp.path().join("manang-live").join(ROOM);
        fs::write(dir.join("junk.loro"), "x").expect("w");
        assert_eq!(s.retain(ROOM, &live), 1);
        // Age-based pruning with a fake "now".
        let now = SystemTime::now();
        assert_eq!(s.prune(Duration::from_secs(60), now), 0);
        let later = now + Duration::from_secs(3600);
        assert_eq!(s.prune(Duration::from_secs(60), later), 3);
        assert!(!tmp.path().join("manang-live/ff").exists());
        // Budget: oldest go first.
        for i in 1..=3u128 {
            s.save(ROOM, FileId(i), &d, &[0u8; 1000]).expect("save");
            let p = dir.join(format!("{}.loro", FileId(i).to_hex()));
            let t = SystemTime::now() - Duration::from_secs(100 - i as u64);
            fs::File::options()
                .write(true)
                .open(&p)
                .and_then(|f| f.set_modified(t))
                .expect("mtime");
        }
        assert_eq!(s.enforce_budget(2200), 1);
        assert_eq!(s.load(ROOM, FileId(1), &d), Rehydrated::Miss);
        assert!(matches!(s.load(ROOM, FileId(3), &d), Rehydrated::Hit(_)));
        s.remove_room(ROOM);
        assert!(!dir.exists());
    }

    #[test]
    fn residency_evicts_least_recently_used_and_honours_pins() {
        let mut r = Residency::new(2);
        assert!(r.touch(1).is_empty());
        assert!(r.touch(2).is_empty());
        assert_eq!(r.touch(3), vec![1]);
        // Touching 2 makes 3 the oldest.
        assert!(r.touch(2).is_empty());
        r.pin(3);
        assert_eq!(r.touch(4), vec![2]);
        assert!(r.contains(&3) && r.contains(&4));
        // Everything pinned: the cap is exceeded rather than evicting.
        r.pin(4);
        r.pin(5);
        assert!(r.touch(5).is_empty());
        assert_eq!(r.len(), 3);
        assert_eq!(r.unpin(&3), vec![3]);
        r.remove(&4);
        assert!(!r.contains(&4));
        assert!(!r.is_empty());
    }
}
