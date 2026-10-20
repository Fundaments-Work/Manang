//! Files the plugin keeps outside the project: the copy a join replaced
//! (`live-trash`, in the config directory) and the disposable document
//! snapshots (`live-snapshots`, in the app data directory).
//!
//! Neither is ever read back without a check, and deleting either loses
//! nothing the person did not already have on disk or in the room.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use manang_live::TextCrdt;
use manang_live::bridge::LiveDoc;
use manang_live::protocol::text_digest;

use crate::error::LiveHostError;

const MAGIC: &[u8; 4] = b"MLS1";
/// Snapshots untouched for this long are removed when a new one is written.
const KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Largest snapshot read back (a guard against a damaged or foreign file).
const MAX_SNAPSHOT: u64 = 256 * 1024 * 1024;

/// Where copies replaced by a join or a resync are saved first.
#[derive(Debug, Clone)]
pub(crate) struct Trash {
    pub dir: PathBuf,
    /// The file's name, made filename-safe when saved.
    pub name: String,
}

impl Trash {
    /// Save `text`; `tag` says why (`join`, `edited`, `resync`).
    pub fn save(&self, text: &str, tag: &str) -> Result<(), LiveHostError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| LiveHostError::Io(e.to_string()))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let safe: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .take(80)
            .collect();
        let name = if tag == "join" {
            format!("{stamp}-{safe}")
        } else {
            format!("{stamp}-{tag}-{safe}")
        };
        std::fs::write(self.dir.join(name), text).map_err(|e| LiveHostError::Io(e.to_string()))
    }
}

/// The directory of document snapshots, one file per room.
#[derive(Debug, Clone)]
pub(crate) struct Snapshots {
    dir: PathBuf,
}

impl Snapshots {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, room_hex: &str) -> Option<PathBuf> {
        // The room id is hex from our own key derivation; refuse anything else
        // so a name can never leave the directory.
        let ok = !room_hex.is_empty() && room_hex.bytes().all(|b| b.is_ascii_hexdigit());
        ok.then(|| self.dir.join(format!("{room_hex}.snap")))
    }

    /// Write the snapshot of `text` for a room, replacing the previous one.
    pub fn save(&self, room_hex: &str, text: &str, snapshot: &[u8]) -> Result<(), LiveHostError> {
        let path = self
            .path(room_hex)
            .ok_or_else(|| LiveHostError::Invalid("bad room id".to_owned()))?;
        std::fs::create_dir_all(&self.dir).map_err(|e| LiveHostError::Io(e.to_string()))?;
        let mut bytes = Vec::with_capacity(snapshot.len() + 12);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&text_digest(text).to_le_bytes());
        bytes.extend_from_slice(snapshot);
        let tmp = path.with_extension("snap.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| LiveHostError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| LiveHostError::Io(e.to_string()))?;
        self.prune(&path);
        Ok(())
    }

    fn prune(&self, keep: &Path) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep || path.extension().is_none_or(|e| e != "snap") {
                continue;
            }
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > KEEP_FOR);
            if old {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// The room's document as it was last saved, but only if it is exactly
    /// the text the file holds now (`disk`) and the snapshot is intact.
    /// Anything else returns `None`: the disk always wins.
    pub fn restore<E: TextCrdt>(
        &self,
        room_hex: &str,
        peer: u64,
        disk: &str,
    ) -> Option<LiveDoc<E>> {
        let path = self.path(room_hex)?;
        if std::fs::metadata(&path).ok()?.len() > MAX_SNAPSHOT {
            return None;
        }
        let bytes = std::fs::read(&path).ok()?;
        let (head, rest) = bytes.split_at_checked(12)?;
        if &head[..4] != MAGIC {
            return None;
        }
        let digest = u64::from_le_bytes(head[4..12].try_into().ok()?);
        if digest != text_digest(disk) {
            return None;
        }
        let engine = E::from_snapshot(peer, rest).ok()?;
        (engine.text() == disk).then(|| LiveDoc::from_engine(engine))
    }
}

#[cfg(test)]
mod tests {
    use manang_live::LoroEngine;

    use super::*;

    fn engine(text: &str) -> LoroEngine {
        LoroEngine::seeded(7, text).expect("engine")
    }

    #[test]
    fn a_snapshot_restores_only_over_identical_disk_text() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = Snapshots::new(dir.path().join("snaps"));
        let e = engine("hello\n");
        store
            .save("abcd01", "hello\n", &e.export_snapshot().expect("snapshot"))
            .expect("save");
        let doc = store
            .restore::<LoroEngine>("abcd01", 9, "hello\n")
            .expect("restores");
        assert_eq!(doc.engine().text(), "hello\n");
        assert!(
            store
                .restore::<LoroEngine>("abcd01", 9, "hello!\n")
                .is_none()
        );
        assert!(store.restore::<LoroEngine>("ffff", 9, "hello\n").is_none());
    }

    #[test]
    fn damaged_or_foreign_files_are_ignored() {
        let dir = tempfile::tempdir().expect("tmp");
        let snaps = dir.path().join("snaps");
        let store = Snapshots::new(snaps.clone());
        std::fs::create_dir_all(&snaps).expect("dir");
        std::fs::write(snaps.join("abcd01.snap"), b"nonsense").expect("write");
        assert!(store.restore::<LoroEngine>("abcd01", 9, "").is_none());
        let e = engine("x");
        store
            .save("abcd02", "x", &e.export_snapshot().expect("snapshot"))
            .expect("save");
        let mut raw = std::fs::read(snaps.join("abcd02.snap")).expect("read");
        let last = raw.len() - 1;
        raw.truncate(last / 2 + 12);
        std::fs::write(snaps.join("abcd02.snap"), raw).expect("write");
        assert!(store.restore::<LoroEngine>("abcd02", 9, "x").is_none());
    }

    #[test]
    fn room_ids_cannot_escape_the_directory() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = Snapshots::new(dir.path().to_path_buf());
        assert!(store.save("../evil", "x", b"y").is_err());
        assert!(store.restore::<LoroEngine>("../evil", 1, "x").is_none());
    }

    #[test]
    fn trash_names_are_filename_safe() {
        let dir = tempfile::tempdir().expect("tmp");
        let trash = Trash {
            dir: dir.path().to_path_buf(),
            name: "../a b/c.typ".to_owned(),
        };
        trash.save("text", "join").expect("save");
        trash.save("text2", "edited").expect("save");
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.iter().all(|n| !n.contains('/') && !n.contains(' ')));
    }
}
