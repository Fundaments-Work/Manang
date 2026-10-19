//! The last verified index, kept on disk so Browse works offline and a
//! conditional request (`If-None-Match`) costs almost nothing.
//!
//! The cache holds the exact bytes of `index.json` and `index.json.sig`, so it
//! is verified again on every load: a tampered cache file is simply ignored.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::RegistryError;

const INDEX_FILE: &str = "index.json";
const SIG_FILE: &str = "index.json.sig";
const META_FILE: &str = "meta.json";
const STATS_FILE: &str = "stats.json";

/// What is known about the cached files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// The registry the files came from; another registry's cache is ignored.
    pub base: String,
    #[serde(default)]
    pub etag: Option<String>,
    /// Unix seconds of the last successful contact with the registry.
    pub fetched_at: u64,
}

/// The cached files.
#[derive(Debug, Clone)]
pub struct Stored {
    pub index: Vec<u8>,
    pub signature: Vec<u8>,
    pub meta: Meta,
}

/// Download counters (`stats.json`): display only, never trusted for
/// anything but ordering and a number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    #[serde(default)]
    pub downloads: BTreeMap<String, u64>,
}

pub const MAX_STATS_BYTES: u64 = 1024 * 1024;

impl Stats {
    /// Tolerant: anything unreadable is "no counters".
    pub fn parse(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_default()
    }
}

/// A cache directory (or nothing, for memory-only use).
#[derive(Debug, Clone, Default)]
pub struct Cache {
    dir: Option<PathBuf>,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> RegistryError + '_ {
    move |source| RegistryError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), RegistryError> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(io(&tmp))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(path)(e)
    })
}

impl Cache {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// The stored files, if all three are there and readable.
    pub fn load(&self) -> Option<Stored> {
        let dir = self.dir.as_ref()?;
        let index = fs::read(dir.join(INDEX_FILE)).ok()?;
        let signature = fs::read(dir.join(SIG_FILE)).ok()?;
        let meta = serde_json::from_slice(&fs::read(dir.join(META_FILE)).ok()?).ok()?;
        Some(Stored {
            index,
            signature,
            meta,
        })
    }

    /// Store verified bytes. Index and signature first, the metadata last: a
    /// crash in between leaves a cache that merely looks older.
    pub fn store(&self, stored: &Stored) -> Result<(), RegistryError> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        fs::create_dir_all(dir).map_err(io(dir))?;
        write_atomic(&dir.join(INDEX_FILE), &stored.index)?;
        write_atomic(&dir.join(SIG_FILE), &stored.signature)?;
        self.store_meta(&stored.meta)
    }

    pub fn store_meta(&self, meta: &Meta) -> Result<(), RegistryError> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        fs::create_dir_all(dir).map_err(io(dir))?;
        let bytes = serde_json::to_vec(meta).map_err(|e| RegistryError::Io {
            path: dir.join(META_FILE),
            source: std::io::Error::other(e),
        })?;
        write_atomic(&dir.join(META_FILE), &bytes)
    }

    pub fn load_stats(&self) -> Stats {
        self.dir
            .as_ref()
            .and_then(|d| fs::read(d.join(STATS_FILE)).ok())
            .map(|b| Stats::parse(&b))
            .unwrap_or_default()
    }

    pub fn store_stats(&self, bytes: &[u8]) {
        if let Some(dir) = &self.dir {
            // Counters are a nicety: failing to keep them is not an error.
            let _ = write_atomic(&dir.join(STATS_FILE), bytes);
        }
    }

    /// Forget everything (a corrupt or foreign cache).
    pub fn clear(&self) {
        if let Some(dir) = &self.dir {
            for name in [INDEX_FILE, SIG_FILE, META_FILE, STATS_FILE] {
                let _ = fs::remove_file(dir.join(name));
            }
        }
    }
}
