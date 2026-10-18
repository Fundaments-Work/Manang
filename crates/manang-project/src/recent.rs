//! Recent projects, persisted as human-readable TOML in the user config dir.
//!
//! ```toml
//! [[projects]]
//! path = "/home/ada/papers/thesis"
//! name = "Thesis"
//! opened_at = 1760000000   # unix seconds
//! ```
//!
//! A missing or unreadable file is treated as an empty list: losing the
//! recents list is harmless, failing to open a project because of it is not.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::ProjectError;
use crate::fsops::atomic_write;
use crate::versioned::load_toml;

/// File name inside the config directory.
pub const RECENT_FILE: &str = "recent-projects.toml";
/// The format version `recent-projects.toml` is written in (`docs/formats.md`).
pub const RECENT_VERSION: u32 = 1;
/// Most entries kept.
pub const MAX_RECENT: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
    pub opened_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFile {
    #[serde(default)]
    projects: Vec<RecentEntry>,
}

/// What is written: the version first, then the list.
#[derive(Serialize)]
struct RecentOut<'a> {
    version: u32,
    projects: &'a [RecentEntry],
}

/// The persisted recent-projects list.
#[derive(Debug)]
pub struct RecentStore {
    file: PathBuf,
    lock: Mutex<()>,
    /// Written by a newer Manang: read-only, never written back.
    newer: Mutex<Option<u32>>,
    warnings: Mutex<Vec<String>>,
}

impl RecentStore {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            file: config_dir.as_ref().join(RECENT_FILE),
            lock: Mutex::new(()),
            newer: Mutex::new(None),
            warnings: Mutex::new(Vec::new()),
        }
    }

    /// Read the list. A damaged file is set aside, never overwritten.
    fn read(&self) -> Vec<RecentEntry> {
        let loaded = load_toml::<RecentFile>(&self.file, RECENT_FILE, RECENT_VERSION, &[]);
        *self.newer.lock() = loaded.newer;
        let mut warnings = self.warnings.lock();
        for warning in loaded.warnings {
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
        loaded.value.projects
    }

    /// Problems found while loading (set-aside, newer format), once each.
    pub fn take_warnings(&self) -> Vec<String> {
        let _guard = self.lock.lock();
        let _ = self.read();
        std::mem::take(&mut self.warnings.lock())
    }

    fn write(&self, projects: Vec<RecentEntry>) -> Result<(), ProjectError> {
        if let Some(found) = *self.newer.lock() {
            return Err(manang_formats::refuse_newer(RECENT_FILE, found, RECENT_VERSION).into());
        }
        let text = toml::to_string_pretty(&RecentOut {
            version: RECENT_VERSION,
            projects: &projects,
        })
        .map_err(|e| ProjectError::io("serialising recent projects", std::io::Error::other(e)))?;
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ProjectError::io("creating the config directory".to_owned(), e))?;
        }
        atomic_write(&self.file, text.as_bytes())
    }

    /// Most recent first.
    pub fn list(&self) -> Vec<RecentEntry> {
        let _guard = self.lock.lock();
        self.read()
    }

    /// Record that `path` was just opened, moving it to the front.
    pub fn touch(&self, path: &Path, name: &str) -> Result<(), ProjectError> {
        let _guard = self.lock.lock();
        let path = path.to_string_lossy().into_owned();
        let mut projects = self.read();
        projects.retain(|p| p.path != path);
        projects.insert(
            0,
            RecentEntry {
                path,
                name: name.to_owned(),
                opened_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
            },
        );
        projects.truncate(MAX_RECENT);
        self.write(projects)
    }

    /// Forget `path`. Returns whether it was present.
    pub fn remove(&self, path: &Path) -> Result<bool, ProjectError> {
        let _guard = self.lock.lock();
        let path = path.to_string_lossy();
        let mut projects = self.read();
        let before = projects.len();
        projects.retain(|p| p.path != path);
        let removed = projects.len() != before;
        if removed {
            self.write(projects)?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_dedupes_orders_and_caps() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RecentStore::new(dir.path());
        assert!(store.list().is_empty());
        for i in 0..(MAX_RECENT + 5) {
            store
                .touch(Path::new(&format!("/p/{i}")), "p")
                .expect("touch");
        }
        store.touch(Path::new("/p/10"), "renamed").expect("touch");
        let list = store.list();
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].path, "/p/10");
        assert_eq!(list[0].name, "renamed");
        assert_eq!(list.iter().filter(|e| e.path == "/p/10").count(), 1);
    }

    #[test]
    fn remove_and_corrupt_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RecentStore::new(dir.path());
        store.touch(Path::new("/a"), "a").expect("touch");
        assert!(store.remove(Path::new("/a")).expect("remove"));
        assert!(!store.remove(Path::new("/a")).expect("remove"));
        fs::write(dir.path().join(RECENT_FILE), "not = [toml").expect("write");
        assert!(store.list().is_empty());
        store.touch(Path::new("/b"), "b").expect("recovers");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn file_is_readable_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RecentStore::new(dir.path());
        store.touch(Path::new("/a"), "A").expect("touch");
        let text = fs::read_to_string(dir.path().join(RECENT_FILE)).expect("read");
        assert!(text.contains("[[projects]]") && text.contains("path = \"/a\""));
    }
}
