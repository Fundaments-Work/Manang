//! The registry client: fetch and verify the index, keep the cache, read
//! text files (readmes) and download plugin archives with size and checksum
//! checks. It never installs anything; the caller hands a verified file to the
//! plugin manager.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cache::{Cache, MAX_STATS_BYTES, Meta, Stats, Stored};
use crate::error::RegistryError;
use crate::index::{Index, MAX_INDEX_BYTES, VersionEntry, parse_index};
use crate::keys::TrustedKeys;
use crate::transport::{Base, Response, Transport};

const INDEX_PATH: &str = "index.json";
const SIG_PATH: &str = "index.json.sig";
const STATS_PATH: &str = "stats.json";
const MAX_SIG_BYTES: u64 = 4096;
const CHUNK: usize = 64 * 1024;

/// How up to date a [`Listing`] is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Freshness {
    /// Downloaded and verified just now.
    Fresh,
    /// The registry confirmed the cached list is current (HTTP 304).
    Unchanged,
    /// The registry could not be reached: this is the last verified list.
    Offline { reason: String },
}

/// A verified index with where it came from.
#[derive(Debug, Clone)]
pub struct Listing {
    pub index: Index,
    pub stats: Stats,
    /// Unix seconds of the last successful contact with the registry.
    pub fetched_at: u64,
    pub freshness: Freshness,
}

pub struct Registry<T: Transport> {
    transport: T,
    base: Base,
    keys: TrustedKeys,
    cache: Cache,
}

fn read_limited(
    mut body: impl Read,
    limit: u64,
    what: &'static str,
) -> Result<Vec<u8>, RegistryError> {
    let mut out = Vec::new();
    (&mut body)
        .take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| RegistryError::Network {
            host: String::new(),
            detail: e.to_string(),
        })?;
    if out.len() as u64 > limit {
        return Err(RegistryError::TooLarge { what, limit });
    }
    Ok(out)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// SHA-256 of `bytes`, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

impl<T: Transport> Registry<T> {
    pub fn new(transport: T, base: Base, keys: TrustedKeys, cache: Cache) -> Self {
        Self {
            transport,
            base,
            keys,
            cache,
        }
    }

    pub fn base(&self) -> &Base {
        &self.base
    }

    fn status_error(&self, status: u16) -> RegistryError {
        RegistryError::BadStatus {
            host: self.base.host().to_owned(),
            status,
        }
    }

    fn get_ok(&self, relative: &str) -> Result<Response, RegistryError> {
        let response = self.transport.get(&self.base.join(relative), None)?;
        if response.status == 200 {
            Ok(response)
        } else {
            Err(self.status_error(response.status))
        }
    }

    /// The cached list, verified again against the trusted keys. A cache that
    /// fails verification, belongs to another registry or cannot be parsed is
    /// treated as absent.
    fn load_verified(&self) -> Option<(Stored, Index)> {
        let stored = self.cache.load()?;
        if stored.meta.base != self.base.as_str() {
            return None;
        }
        self.keys.verify(&stored.index, &stored.signature).ok()?;
        let index = parse_index(&stored.index).ok()?;
        Some((stored, index))
    }

    /// The last verified list, without touching the network.
    pub fn cached(&self) -> Option<Listing> {
        let (stored, index) = self.load_verified()?;
        Some(Listing {
            index,
            stats: self.cache.load_stats(),
            fetched_at: stored.meta.fetched_at,
            freshness: Freshness::Offline {
                reason: "not checked yet".to_owned(),
            },
        })
    }

    /// Ask the registry for the current list (conditionally, when a cache
    /// exists). An unreachable registry falls back to the cache; a list that
    /// fails verification, is older than the cached one, or is malformed is an
    /// error and the cache is left as it was.
    pub fn refresh(&self, now: u64) -> Result<Listing, RegistryError> {
        if self.keys.is_empty() {
            return Err(RegistryError::NotConfigured);
        }
        let cached = self.load_verified();
        match self.fetch(now, cached.as_ref()) {
            Ok(listing) => Ok(listing),
            Err(e) if e.is_unreachable() => match cached {
                Some((stored, index)) => Ok(Listing {
                    index,
                    stats: self.cache.load_stats(),
                    fetched_at: stored.meta.fetched_at,
                    freshness: Freshness::Offline {
                        reason: e.to_string(),
                    },
                }),
                None => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    fn fetch(&self, now: u64, cached: Option<&(Stored, Index)>) -> Result<Listing, RegistryError> {
        let etag = cached.and_then(|(s, _)| s.meta.etag.as_deref());
        let response = self.transport.get(&self.base.join(INDEX_PATH), etag)?;
        match response.status {
            304 => {
                let Some((stored, index)) = cached else {
                    return Err(self.status_error(304));
                };
                let meta = Meta {
                    fetched_at: now,
                    ..stored.meta.clone()
                };
                // Failing to note the visit is not worth failing the listing.
                let _ = self.cache.store_meta(&meta);
                Ok(Listing {
                    index: index.clone(),
                    stats: self.stats(),
                    fetched_at: now,
                    freshness: Freshness::Unchanged,
                })
            }
            200 => {
                let new_etag = response.etag.clone();
                let bytes = read_limited(response.body, MAX_INDEX_BYTES, "the plugin list")?;
                let sig_response = self.get_ok(SIG_PATH)?;
                let signature = read_limited(sig_response.body, MAX_SIG_BYTES, "the signature")?;
                self.keys.verify(&bytes, &signature)?;
                let index = parse_index(&bytes)?;
                if let Some((_, old)) = cached
                    && index.generated < old.generated
                {
                    return Err(RegistryError::Stale);
                }
                self.cache.store(&Stored {
                    index: bytes,
                    signature,
                    meta: Meta {
                        base: self.base.as_str().to_owned(),
                        etag: new_etag,
                        fetched_at: now,
                    },
                })?;
                Ok(Listing {
                    index,
                    stats: self.stats(),
                    fetched_at: now,
                    freshness: Freshness::Fresh,
                })
            }
            other => Err(self.status_error(other)),
        }
    }

    /// Download counters, best effort: the cached ones when the file cannot
    /// be fetched.
    fn stats(&self) -> Stats {
        let fetched = self
            .get_ok(STATS_PATH)
            .and_then(|r| read_limited(r.body, MAX_STATS_BYTES, "the statistics"));
        match fetched {
            Ok(bytes) => {
                self.cache.store_stats(&bytes);
                Stats::parse(&bytes)
            }
            Err(_) => self.cache.load_stats(),
        }
    }

    /// A small text file below the base (a readme), at most `limit` bytes.
    /// `relative` must come from the verified index.
    pub fn fetch_text(&self, relative: &str, limit: u64) -> Result<String, RegistryError> {
        let response = self.get_ok(relative)?;
        let bytes = read_limited(response.body, limit, "the file")?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Download `version`'s archive to `dest`, checking the size and SHA-256
    /// the verified index lists. On any failure `dest` is removed. `progress`
    /// receives `(received, expected)`; `cancel` stops the transfer.
    pub fn download(
        &self,
        version: &VersionEntry,
        dest: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), RegistryError> {
        let result = self.download_inner(version, dest, cancel, progress);
        if result.is_err() {
            // Our own partial file.
            let _ = fs::remove_file(dest);
        }
        result
    }

    fn download_inner(
        &self,
        version: &VersionEntry,
        dest: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), RegistryError> {
        let expected = version.size;
        let mut response = self.get_ok(&version.download)?;
        let io = |source| RegistryError::Io {
            path: dest.to_path_buf(),
            source,
        };
        let mut file = fs::File::create(dest).map_err(io)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK];
        let mut received = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(RegistryError::Cancelled);
            }
            let n = response
                .body
                .read(&mut buf)
                .map_err(|_| RegistryError::Truncated { received, expected })?;
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > expected {
                return Err(RegistryError::SizeMismatch);
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(io)?;
            progress(received, expected);
        }
        file.sync_all().map_err(io)?;
        if received != expected {
            return Err(RegistryError::SizeMismatch);
        }
        if !hex(&hasher.finalize()).eq_ignore_ascii_case(&version.sha256) {
            return Err(RegistryError::HashMismatch);
        }
        Ok(())
    }
}
