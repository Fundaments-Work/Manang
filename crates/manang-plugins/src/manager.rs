use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use manang_kernel::manifest::MAX_MANIFEST_BYTES;
use manang_kernel::{Kernel, Manifest};
use manang_wasm_host::{
    CacheStatus, HostServices, Limits, RuntimeOptions, WasmPlugin, WasmRuntime,
};
use parking_lot::Mutex;
use rayon::prelude::*;

use crate::error::{LoadError, LoadFailure, ManagerError};

/// File names inside a plugin directory.
pub const MANIFEST_FILE: &str = "manifest.toml";
pub const DEFAULT_ENTRY: &str = "plugin.wasm";
/// Largest `.wasm` the manager will read.
pub const MAX_WASM_BYTES: u64 = 64 * 1024 * 1024;

/// `<config>/manang/plugins`.
pub fn plugins_dir_in(config_dir: &Path) -> PathBuf {
    config_dir.join("manang").join("plugins")
}

/// `<cache>/manang/wasm`, the default home of the precompiled-component cache.
pub fn cache_dir_in(cache_dir: &Path) -> PathBuf {
    cache_dir.join("manang").join("wasm")
}

/// How the manager finds and hosts plugins.
#[derive(Clone)]
pub struct ManagerConfig {
    /// Holds one directory per plugin: `<name>/{manifest.toml, plugin.wasm}`.
    pub plugins_dir: PathBuf,
    /// Precompiled-component cache. `None` compiles on every load.
    pub cache_dir: Option<PathBuf>,
    pub limits: Limits,
    /// Use the pooling allocator.
    pub pooling: bool,
    /// Services plugins may be granted (buffer, project directory).
    pub services: HostServices,
}

impl ManagerConfig {
    pub fn new(plugins_dir: impl Into<PathBuf>) -> Self {
        Self {
            plugins_dir: plugins_dir.into(),
            cache_dir: None,
            limits: Limits::default(),
            pooling: true,
            services: HostServices::default(),
        }
    }
}

/// A plugin the manager has loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub dir: PathBuf,
    /// Whether the component came from the precompiled cache.
    pub cache: CacheStatus,
}

/// What a [`PluginManager::sync`] did.
#[derive(Debug)]
pub enum SyncOutcome {
    Loaded(PluginInfo),
    Reloaded(PluginInfo),
    /// Files are byte-identical to what is already running.
    Unchanged(String),
    Unloaded(String),
    /// Nothing on disk and nothing loaded.
    Absent,
    /// The load failed. If a plugin of that name was running it keeps running.
    Failed(LoadFailure),
}

/// Result of scanning the plugins directory.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub loaded: Vec<PluginInfo>,
    pub failed: Vec<LoadFailure>,
}

struct Loaded {
    info: PluginInfo,
    fingerprint: blake3::Hash,
}

struct Prepared {
    manifest: Manifest,
    plugin: WasmPlugin,
    info: PluginInfo,
    fingerprint: blake3::Hash,
}

/// Discovers, loads, unloads and hot-reloads WASM plugins into a [`Kernel`].
///
/// Loading compiles components, which is CPU-bound: call from a Rayon or
/// blocking thread, never from an async worker.
pub struct PluginManager {
    kernel: Arc<Kernel>,
    runtime: WasmRuntime,
    services: HostServices,
    root: PathBuf,
    loaded: Mutex<HashMap<String, Loaded>>,
    /// Serialises registry mutations. Calls into plugins never take it.
    ops: Mutex<()>,
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> LoadError + '_ {
    move |source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Read a file no larger than `limit`.
fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>, LoadError> {
    let file = fs::File::open(path).map_err(io_error(path))?;
    let meta = file.metadata().map_err(io_error(path))?;
    if !meta.is_file() {
        return Err(LoadError::NoManifest(path.to_path_buf()));
    }
    if meta.len() > limit {
        return Err(LoadError::TooLarge {
            path: path.to_path_buf(),
            size: meta.len(),
            limit,
        });
    }
    let mut out = Vec::with_capacity(meta.len() as usize);
    file.take(limit + 1)
        .read_to_end(&mut out)
        .map_err(io_error(path))?;
    if out.len() as u64 > limit {
        return Err(LoadError::TooLarge {
            path: path.to_path_buf(),
            size: out.len() as u64,
            limit,
        });
    }
    Ok(out)
}

impl PluginManager {
    /// Create the manager. Creates `plugins_dir` if it does not exist.
    pub fn new(kernel: Arc<Kernel>, config: ManagerConfig) -> Result<Self, ManagerError> {
        let io = |source| ManagerError::Io {
            path: config.plugins_dir.clone(),
            source,
        };
        fs::create_dir_all(&config.plugins_dir).map_err(io)?;
        let root = fs::canonicalize(&config.plugins_dir).map_err(io)?;
        let runtime = WasmRuntime::with_options(RuntimeOptions {
            limits: config.limits,
            pooling: config.pooling,
            cache_dir: config.cache_dir,
            ..RuntimeOptions::default()
        })?;
        Ok(Self {
            kernel,
            runtime,
            services: config.services,
            root,
            loaded: Mutex::new(HashMap::new()),
            ops: Mutex::new(()),
        })
    }

    /// The (canonical) plugins directory.
    pub fn plugins_dir(&self) -> &Path {
        &self.root
    }

    /// The kernel plugins are registered with.
    pub fn kernel(&self) -> &Arc<Kernel> {
        &self.kernel
    }

    /// Loaded plugins, sorted by name.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        let mut out: Vec<_> = self
            .loaded
            .lock()
            .values()
            .map(|l| l.info.clone())
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Plugin directories that could hold a plugin: visible sub-directories,
    /// sorted so hook order is deterministic.
    fn candidates(&self) -> Result<Vec<PathBuf>, ManagerError> {
        let read = fs::read_dir(&self.root).map_err(|source| ManagerError::Io {
            path: self.root.clone(),
            source,
        })?;
        let mut dirs: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        dirs.sort();
        Ok(dirs)
    }

    /// Read, validate and compile one plugin directory without touching the
    /// registry. `Ok(None)` when the files are identical to what is loaded.
    fn prepare(&self, dir: &Path) -> Result<Option<Prepared>, Box<LoadFailure>> {
        let fail = |plugin: Option<String>, error: LoadError| {
            Box::new(LoadFailure {
                dir: dir.to_path_buf(),
                plugin,
                error,
            })
        };
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest_bytes =
            read_capped(&manifest_path, MAX_MANIFEST_BYTES as u64).map_err(|e| match e {
                LoadError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                    fail(None, LoadError::NoManifest(dir.to_path_buf()))
                }
                other => fail(None, other),
            })?;
        let source = String::from_utf8(manifest_bytes.clone()).map_err(|_| {
            fail(
                None,
                LoadError::Io {
                    path: manifest_path.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "manifest is not valid UTF-8",
                    ),
                },
            )
        })?;
        let manifest = Manifest::from_toml(&source).map_err(|e| fail(None, e.into()))?;
        let name = manifest.plugin.name.clone();
        let fail = |error: LoadError| fail(Some(name.clone()), error);

        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if dir_name != name {
            return Err(fail(LoadError::NameMismatch {
                dir: dir_name,
                name: name.clone(),
            }));
        }

        let entry = manifest.plugin.entry.as_deref().unwrap_or(DEFAULT_ENTRY);
        let canonical_dir = fs::canonicalize(dir).map_err(|e| fail(io_error(dir)(e)))?;
        let entry_path = fs::canonicalize(canonical_dir.join(entry))
            .map_err(|e| fail(io_error(&canonical_dir.join(entry))(e)))?;
        if !entry_path.starts_with(&canonical_dir) {
            return Err(fail(LoadError::EntryEscapes(entry.to_owned())));
        }
        let wasm = read_capped(&entry_path, MAX_WASM_BYTES).map_err(&fail)?;

        let mut hasher = blake3::Hasher::new();
        hasher.update(&(manifest_bytes.len() as u64).to_le_bytes());
        hasher.update(&manifest_bytes);
        hasher.update(&wasm);
        let fingerprint = hasher.finalize();
        if self
            .loaded
            .lock()
            .get(&name)
            .is_some_and(|l| l.fingerprint == fingerprint)
        {
            return Ok(None);
        }

        let plugin = self
            .runtime
            .load_with(&manifest, &wasm, self.services.clone())
            .map_err(|e| fail(e.into()))?;
        let info = PluginInfo {
            name: name.clone(),
            version: manifest.plugin.version.to_string(),
            description: manifest.plugin.description.clone(),
            dir: dir.to_path_buf(),
            cache: plugin.cache_status().clone(),
        };
        Ok(Some(Prepared {
            manifest,
            plugin,
            info,
            fingerprint,
        }))
    }

    /// Register a prepared plugin, replacing a running one of the same name.
    fn commit(&self, prepared: Prepared) -> SyncOutcome {
        let _ops = self.ops.lock();
        let Prepared {
            manifest,
            plugin,
            info,
            fingerprint,
        } = prepared;
        let name = info.name.clone();
        let reloading = self.loaded.lock().contains_key(&name);
        let result = if reloading {
            self.kernel.replace(manifest, Arc::new(plugin))
        } else {
            self.kernel.register(manifest, Arc::new(plugin))
        };
        match result {
            Ok(()) => {
                self.loaded.lock().insert(
                    name,
                    Loaded {
                        info: info.clone(),
                        fingerprint,
                    },
                );
                if reloading {
                    SyncOutcome::Reloaded(info)
                } else {
                    SyncOutcome::Loaded(info)
                }
            }
            Err(e) => SyncOutcome::Failed(LoadFailure {
                dir: info.dir,
                plugin: Some(name),
                error: e.into(),
            }),
        }
    }

    /// Load every plugin directory, compiling in parallel. A broken plugin is
    /// reported in [`ScanReport::failed`] and does not stop the rest. Already
    /// running plugins whose files changed are reloaded.
    pub fn scan(&self) -> Result<ScanReport, ManagerError> {
        let dirs = self.candidates()?;
        let prepared: Vec<_> = dirs.par_iter().map(|dir| self.prepare(dir)).collect();
        let mut report = ScanReport::default();
        for result in prepared {
            match result {
                Ok(Some(p)) => match self.commit(p) {
                    SyncOutcome::Loaded(i) | SyncOutcome::Reloaded(i) => report.loaded.push(i),
                    SyncOutcome::Failed(f) => report.failed.push(f),
                    _ => {}
                },
                Ok(None) => {}
                Err(f) => report.failed.push(*f),
            }
        }
        Ok(report)
    }

    /// Bring the plugin named `name` in line with its directory: load it if
    /// new, hot-reload it if changed, unload it if the directory is gone. A
    /// failed reload leaves the running plugin untouched.
    pub fn sync(&self, name: &str) -> SyncOutcome {
        // Only plain directory names; never follow a path out of the root.
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            return SyncOutcome::Absent;
        }
        let dir = self.root.join(name);
        if !dir.is_dir() {
            return match self.unload(name) {
                Ok(()) => SyncOutcome::Unloaded(name.to_owned()),
                Err(_) => SyncOutcome::Absent,
            };
        }
        match self.prepare(&dir) {
            Ok(Some(p)) => self.commit(p),
            Ok(None) => SyncOutcome::Unchanged(name.to_owned()),
            Err(f) => SyncOutcome::Failed(*f),
        }
    }

    /// Re-read a loaded plugin from disk (same as [`PluginManager::sync`] but
    /// an error if nothing by that name is loaded).
    pub fn reload(&self, name: &str) -> Result<SyncOutcome, ManagerError> {
        if !self.loaded.lock().contains_key(name) {
            return Err(ManagerError::NotLoaded(name.to_owned()));
        }
        Ok(self.sync(name))
    }

    /// Unregister a plugin. Calls already running finish on it; its commands
    /// and hooks disappear immediately. The files are left alone.
    pub fn unload(&self, name: &str) -> Result<(), ManagerError> {
        let _ops = self.ops.lock();
        if self.loaded.lock().remove(name).is_none() {
            return Err(ManagerError::NotLoaded(name.to_owned()));
        }
        self.kernel.unregister(name)?;
        Ok(())
    }
}
