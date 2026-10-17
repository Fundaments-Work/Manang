//! Tamper-evident cache of precompiled components (`.cwasm`).
//!
//! Compiling a component costs hundreds of milliseconds; deserialising a
//! precompiled artefact costs a few. `Component::deserialize` is `unsafe`
//! because it trusts its input to be machine code Wasmtime itself produced, so
//! this cache only ever feeds it bytes that carry a valid keyed MAC.
//!
//! * **Key.** `<blake3(wasm)>-<engine fingerprint>.cwasm`. The fingerprint
//!   hashes Wasmtime's own compatibility hash (compiler settings, tunables,
//!   enabled features) and this crate's version, so a different engine
//!   configuration never reuses an artefact. Wasmtime additionally rejects any
//!   artefact built by a different Wasmtime version when deserialising.
//! * **Integrity.** Each file is `magic | mac | artefact`. The MAC is BLAKE3
//!   keyed with a random 32-byte secret in `cache.key` (created on first use,
//!   mode 0600), over the fingerprint, the content hash and the artefact. A
//!   flipped bit, a truncated file, or an artefact copied from another plugin's
//!   entry fails verification and is recompiled and overwritten.
//! * **Threat model.** This defends against corruption and against writers
//!   that can write the cache directory but not read `cache.key`. An attacker
//!   running as the same user who can read the key could also modify the
//!   binary itself, so that case is out of scope.
//!
//! Cache trouble is never fatal: any read, verify or deserialise failure falls
//! back to compiling from the wasm bytes.

use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use wasmtime::Engine;
use wasmtime::component::Component;

use crate::error::CacheError;

const MAGIC: &[u8; 8] = b"MNGCWASM";
const MAC_LEN: usize = 32;
const HEADER_LEN: usize = MAGIC.len() + MAC_LEN;
const KEY_LEN: usize = 32;
/// Refuse to read artefacts larger than this.
const MAX_ARTEFACT_BYTES: u64 = 512 * 1024 * 1024;
/// Entries kept after pruning, newest first.
const MAX_ENTRIES: usize = 256;

/// How a component was obtained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheStatus {
    /// No cache is configured.
    Disabled,
    /// Deserialised from a verified cache entry.
    Hit,
    /// Compiled and stored.
    Miss,
    /// A cache entry existed but failed verification or deserialisation (the
    /// reason is attached); the component was recompiled and the entry
    /// replaced.
    Rejected(String),
}

/// Precompiled-component cache rooted at one directory.
#[derive(Debug)]
pub struct ComponentCache {
    dir: PathBuf,
    key: [u8; KEY_LEN],
}

/// Bytes that are safe to hand to `Component::deserialize`: either they carry a
/// valid MAC from this cache's key, or this process just produced them with
/// `Engine::precompile_component`. The only constructors are in this module.
struct TrustedArtefact(Vec<u8>);

/// Deserialise an artefact the host itself produced.
///
/// This is the single `unsafe` operation in the crate.
#[allow(unsafe_code)] // Required: `Component::deserialize` is `unsafe`; see SAFETY.
fn deserialize_trusted(engine: &Engine, artefact: &TrustedArtefact) -> wasmtime::Result<Component> {
    // SAFETY: `TrustedArtefact` can only be built from (a) the output of
    // `Engine::precompile_component` in this process, or (b) a cache file whose
    // keyed BLAKE3 MAC verified under a secret only this host's user can read.
    // Both are "previously produced by `Engine::precompile_component`", which
    // is the only requirement `Component::deserialize` documents. Wasmtime
    // still checks version and configuration compatibility itself.
    unsafe { Component::deserialize(engine, &artefact.0) }
}

impl ComponentCache {
    /// Open (creating if needed) a cache in `dir`.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let dir = dir.into();
        let io = |path: &Path, source| CacheError::Io {
            path: path.display().to_string(),
            source,
        };
        fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
        let key_path = dir.join("cache.key");
        let key = load_or_create_key(&key_path)?;
        Ok(Self { dir, key })
    }

    /// The directory holding the artefacts.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn fingerprint(engine: &Engine) -> String {
        let mut hasher = std::hash::DefaultHasher::new();
        engine.precompile_compatibility_hash().hash(&mut hasher);
        let mut out = blake3::Hasher::new();
        out.update(&hasher.finish().to_le_bytes());
        out.update(env!("CARGO_PKG_VERSION").as_bytes());
        out.finalize().to_hex()[..16].to_owned()
    }

    fn mac(&self, fingerprint: &str, content: &blake3::Hash, artefact: &[u8]) -> blake3::Hash {
        let mut h = blake3::Hasher::new_keyed(&self.key);
        h.update(fingerprint.as_bytes());
        h.update(content.as_bytes());
        h.update(artefact);
        h.finalize()
    }

    /// Read and verify one entry. `Ok(None)` when absent.
    fn read(
        &self,
        path: &Path,
        fingerprint: &str,
        content: &blake3::Hash,
    ) -> Result<Option<TrustedArtefact>, String> {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("cannot stat entry: {e}")),
        };
        if !meta.is_file() || meta.len() > MAX_ARTEFACT_BYTES {
            return Err("entry is not a regular file of sane size".to_owned());
        }
        let bytes = fs::read(path).map_err(|e| format!("cannot read entry: {e}"))?;
        if bytes.len() < HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
            return Err("entry has no valid header".to_owned());
        }
        let (mac, artefact) = bytes[MAGIC.len()..].split_at(MAC_LEN);
        let expected = self.mac(fingerprint, content, artefact);
        // `blake3::Hash` equality is constant-time.
        let mut given = [0u8; MAC_LEN];
        given.copy_from_slice(mac);
        if blake3::Hash::from_bytes(given) != expected {
            return Err("MAC mismatch (corrupted or tampered entry)".to_owned());
        }
        Ok(Some(TrustedArtefact(artefact.to_vec())))
    }

    fn write(&self, path: &Path, mac: &blake3::Hash, artefact: &[u8]) -> std::io::Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let tmp = self.dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(MAGIC)?;
            file.write_all(mac.as_bytes())?;
            file.write_all(artefact)?;
            file.sync_all()?;
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    /// Delete the oldest entries beyond [`MAX_ENTRIES`]. Best effort.
    fn prune(&self) {
        let Ok(read) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut entries: Vec<(SystemTime, PathBuf)> = read
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "cwasm"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if entries.len() <= MAX_ENTRIES {
            return;
        }
        entries.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (_, path) in entries.into_iter().skip(MAX_ENTRIES) {
            let _ = fs::remove_file(path);
        }
    }

    /// Return the component for `wasm`, from cache when a verified entry
    /// exists, otherwise compiling and storing it.
    pub(crate) fn load_or_compile(
        &self,
        engine: &Engine,
        wasm: &[u8],
    ) -> wasmtime::Result<(Component, CacheStatus)> {
        let content = blake3::hash(wasm);
        let fingerprint = Self::fingerprint(engine);
        let path = self
            .dir
            .join(format!("{}-{fingerprint}.cwasm", content.to_hex()));

        let mut status = CacheStatus::Miss;
        match self.read(&path, &fingerprint, &content) {
            Ok(Some(artefact)) => match deserialize_trusted(engine, &artefact) {
                Ok(component) => return Ok((component, CacheStatus::Hit)),
                Err(e) => status = CacheStatus::Rejected(format!("deserialise failed: {e}")),
            },
            Ok(None) => {}
            Err(reason) => status = CacheStatus::Rejected(reason),
        }

        let bytes = engine.precompile_component(wasm)?;
        let mac = self.mac(&fingerprint, &content, &bytes);
        // A failed write only costs the next start its speed-up.
        if self.write(&path, &mac, &bytes).is_ok() {
            self.prune();
        }
        let component = deserialize_trusted(engine, &TrustedArtefact(bytes))?;
        Ok((component, status))
    }
}

fn load_or_create_key(path: &Path) -> Result<[u8; KEY_LEN], CacheError> {
    let io = |source| CacheError::Io {
        path: path.display().to_string(),
        source,
    };
    let read = |path: &Path| -> Result<[u8; KEY_LEN], CacheError> {
        let bytes = fs::read(path).map_err(io)?;
        bytes.try_into().map_err(|_| CacheError::BadKey {
            path: path.display().to_string(),
            expected: KEY_LEN,
        })
    };
    if path.exists() {
        return read(path);
    }
    let mut key = [0u8; KEY_LEN];
    getrandom::fill(&mut key).map_err(|e| CacheError::Random(e.to_string()))?;
    // Write a private temp file, then publish it with an atomic hard link so a
    // concurrent opener never sees a half-written key.
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).map_err(io)?;
    let written = file.write_all(&key).and_then(|()| file.sync_all());
    drop(file);
    let linked = written.and_then(|()| fs::hard_link(&tmp, path));
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(key),
        // Another process won the race; use its key.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read(path),
        Err(e) => Err(io(e)),
    }
}
