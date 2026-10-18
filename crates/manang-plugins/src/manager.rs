use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use manang_kernel::manifest::{MAX_MANIFEST_BYTES, Permissions};
use manang_kernel::{Kernel, Manifest, Plugin, Tier};
use manang_wasm_host::{
    CacheStatus, HostServices, Limits, RuntimeOptions, WasmPlugin, WasmRuntime,
};
use parking_lot::Mutex;
use rayon::prelude::*;

use crate::error::{LoadError, LoadFailure, ManagerError};
use crate::events::{Listener, ManagerEvent};
use crate::perms;
use crate::policy::{Permissive, PolicyAction, PolicyGate, PolicyRequest};
use crate::registry::{ProjectOverride, Registry, RegistryError};

/// File names inside a plugin directory.
pub const MANIFEST_FILE: &str = "manifest.toml";
pub const DEFAULT_ENTRY: &str = "plugin.wasm";
/// Largest `.wasm` the manager will read.
pub const MAX_WASM_BYTES: u64 = 64 * 1024 * 1024;

/// First-party plugins the app cannot run without: they can be neither
/// disabled nor removed. (Everything else built in may be switched off.)
pub const CORE_PLUGINS: &[&str] = &[
    "editor",
    "project",
    "settings",
    "keybindings",
    "theme",
    "plugins",
];

/// `<config>/manang/plugins`.
pub fn plugins_dir_in(config_dir: &Path) -> PathBuf {
    config_dir.join("manang").join("plugins")
}

/// `<cache>/manang/wasm`, the default home of the precompiled-component cache.
pub fn cache_dir_in(cache_dir: &Path) -> PathBuf {
    cache_dir.join("manang").join("wasm")
}

/// `<config>/manang/plugins.toml`, the user's plugin decisions.
pub fn registry_path_in(config_dir: &Path) -> PathBuf {
    config_dir.join("manang").join("plugins.toml")
}

/// `<config>/manang/plugins-trash`, where uninstalled plugins are moved.
pub fn trash_dir_in(config_dir: &Path) -> PathBuf {
    config_dir.join("manang").join("plugins-trash")
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
    /// Where the user's decisions live (`plugins.toml`). `None` keeps them in
    /// memory only.
    pub registry_path: Option<PathBuf>,
    /// Where uninstalled plugins go. `None`: `<plugins_dir>/.trash`.
    pub trash_dir: Option<PathBuf>,
    /// Built-in plugins that cannot be disabled or removed.
    pub core: BTreeSet<String>,
    /// Consulted before installing and enabling.
    pub policy: Arc<dyn PolicyGate>,
    /// Told about every change.
    pub listener: Option<Listener>,
}

impl ManagerConfig {
    pub fn new(plugins_dir: impl Into<PathBuf>) -> Self {
        Self {
            plugins_dir: plugins_dir.into(),
            cache_dir: None,
            limits: Limits::default(),
            pooling: true,
            services: HostServices::default(),
            registry_path: None,
            trash_dir: None,
            core: CORE_PLUGINS.iter().map(|s| (*s).to_owned()).collect(),
            policy: Arc::new(Permissive),
            listener: None,
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
    /// The plugin is switched off (by the user or the project). If it was
    /// running it has been unloaded.
    Disabled {
        name: String,
        unloaded: bool,
    },
    /// The plugin asks for permissions nobody approved. It is not loaded; a
    /// running older version keeps running.
    NeedsApproval {
        name: String,
        version: String,
    },
    /// The load failed. If a plugin of that name was running it keeps running.
    Failed(LoadFailure),
}

/// Result of scanning the plugins directory.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub loaded: Vec<PluginInfo>,
    /// Plugins left unloaded because they await approval or are disabled.
    pub held: Vec<String>,
    pub failed: Vec<LoadFailure>,
}

pub(crate) struct Loaded {
    pub(crate) info: PluginInfo,
    pub(crate) fingerprint: blake3::Hash,
}

/// A failure kept for the Plugins view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailureRecord {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
    pub(crate) error: String,
}

/// A plugin held back from loading until the user approves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Held {
    pub(crate) version: String,
}

/// A built-in plugin taken out of service: its manifest and the instance.
pub(crate) type BuiltIn = (Manifest, Arc<dyn Plugin>);

/// Who switched a plugin off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisabledBy {
    User,
    Project,
}

struct Prepared {
    manifest: Manifest,
    plugin: WasmPlugin,
    info: PluginInfo,
    fingerprint: blake3::Hash,
    /// No record existed and nothing was requested: record it as approved.
    auto_approve: bool,
}

enum Prep {
    Unchanged(String),
    Disabled(String),
    Held { name: String, version: String },
    Load(Box<Prepared>),
}

enum Approval {
    /// Load with these permissions (the requested set the user approved).
    Granted(Permissions),
    /// Nothing requested and never recorded: side-loaded, approve silently.
    Auto,
    Held,
}

/// The manifest and compiled-entry bytes of a plugin folder, validated but not
/// compiled.
pub(crate) struct Candidate {
    pub(crate) manifest: Manifest,
    pub(crate) wasm: Vec<u8>,
}

/// Discovers, loads, unloads and hot-reloads WASM plugins into a [`Kernel`],
/// and manages their lifecycle: install, approve, enable, disable, uninstall.
///
/// Loading compiles components, which is CPU-bound: call from a Rayon or
/// blocking thread, never from an async worker.
pub struct PluginManager {
    pub(crate) kernel: Arc<Kernel>,
    pub(crate) runtime: WasmRuntime,
    pub(crate) services: HostServices,
    pub(crate) root: PathBuf,
    pub(crate) trash: PathBuf,
    pub(crate) core: BTreeSet<String>,
    pub(crate) policy: Arc<dyn PolicyGate>,
    listener: Option<Listener>,
    pub(crate) loaded: Mutex<HashMap<String, Loaded>>,
    pub(crate) registry: Mutex<Registry>,
    pub(crate) project: Mutex<ProjectOverride>,
    pub(crate) failures: Mutex<BTreeMap<String, FailureRecord>>,
    pub(crate) held: Mutex<BTreeMap<String, Held>>,
    /// Built-in plugins taken out of service, kept to put back.
    pub(crate) natives_off: Mutex<BTreeMap<String, BuiltIn>>,
    pub(crate) warnings: Mutex<Vec<String>>,
    /// Serialises registry mutations. Calls into plugins never take it.
    ops: Mutex<()>,
    /// Serialises lifecycle operations (install, approve, ...). Taken before
    /// `ops`; never held by the file watcher.
    pub(crate) admin: Mutex<()>,
}

pub(crate) fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> LoadError + '_ {
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

/// Seconds since the Unix epoch (0 if the clock is before it).
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Read and parse `<dir>/manifest.toml`.
pub(crate) fn read_manifest(dir: &Path) -> Result<(Manifest, Vec<u8>), LoadError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES as u64).map_err(|e| match e {
        LoadError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            LoadError::NoManifest(dir.to_path_buf())
        }
        other => other,
    })?;
    let source = std::str::from_utf8(&bytes).map_err(|_| LoadError::Io {
        path: manifest_path.clone(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "manifest is not valid UTF-8",
        ),
    })?;
    let manifest = Manifest::from_toml(source)?;
    Ok((manifest, bytes))
}

/// Read the plugin's entry file; it must lie inside `dir`.
pub(crate) fn read_entry(dir: &Path, manifest: &Manifest) -> Result<Vec<u8>, LoadError> {
    let entry = manifest.plugin.entry.as_deref().unwrap_or(DEFAULT_ENTRY);
    let canonical_dir = fs::canonicalize(dir).map_err(io_error(dir))?;
    let wanted = canonical_dir.join(entry);
    let entry_path = fs::canonicalize(&wanted).map_err(io_error(&wanted))?;
    if !entry_path.starts_with(&canonical_dir) {
        return Err(LoadError::EntryEscapes(entry.to_owned()));
    }
    read_capped(&entry_path, MAX_WASM_BYTES)
}

impl PluginManager {
    /// Create the manager. Creates `plugins_dir` if it does not exist and
    /// reads the user's decisions from `registry_path`. A damaged registry is
    /// set aside as `<file>.corrupt` and reported in [`PluginManager::warnings`].
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
        let mut warnings = Vec::new();
        let registry = match &config.registry_path {
            None => Registry::ephemeral(),
            Some(path) => match Registry::load(path) {
                Ok(r) => r,
                Err(e @ (RegistryError::Parse { .. } | RegistryError::BadName(..))) => {
                    // Never overwritten: an earlier .corrupt copy is kept too.
                    warnings.push(match manang_formats::set_aside_corrupt(path) {
                        Ok(to) => format!("{e}; it was kept as {} and reset", to.display()),
                        Err(re) => format!("{e}; and it could not be set aside: {re}"),
                    });
                    Registry::empty_at(path)
                }
                Err(e) => return Err(e.into()),
            },
        };
        if let Some(found) = registry.newer_version() {
            warnings.push(format!(
                "plugins.toml was written by a newer Manang (format version {found}); your plugin decisions are read as far as understood and not changed until you update"
            ));
        }
        let trash = config.trash_dir.unwrap_or_else(|| root.join(".trash"));
        Ok(Self {
            kernel,
            runtime,
            services: config.services,
            root,
            trash,
            core: config.core,
            policy: config.policy,
            listener: config.listener,
            loaded: Mutex::new(HashMap::new()),
            registry: Mutex::new(registry),
            project: Mutex::new(ProjectOverride::default()),
            failures: Mutex::new(BTreeMap::new()),
            held: Mutex::new(BTreeMap::new()),
            natives_off: Mutex::new(BTreeMap::new()),
            warnings: Mutex::new(warnings),
            ops: Mutex::new(()),
            admin: Mutex::new(()),
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

    /// Problems found at startup (a damaged registry, ...), for the UI.
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.lock().clone()
    }

    pub(crate) fn warn(&self, message: String) {
        let mut w = self.warnings.lock();
        if w.len() < 20 && !w.contains(&message) {
            w.push(message);
        }
    }

    pub(crate) fn emit(&self, event: ManagerEvent) {
        if let Some(listener) = &self.listener {
            listener(&event);
        }
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

    /// Whether the user or the open project has switched `name` off.
    pub fn disabled_by(&self, name: &str) -> Option<DisabledBy> {
        if self.registry.lock().get(name).is_some_and(|r| !r.enabled) {
            return Some(DisabledBy::User);
        }
        self.project
            .lock()
            .disables(name)
            .then_some(DisabledBy::Project)
    }

    fn approval(&self, name: &str, manifest: &Manifest) -> Approval {
        let requested = &manifest.permissions;
        let reg = self.registry.lock();
        match reg.get(name) {
            None if perms::is_empty(requested) => Approval::Auto,
            None => Approval::Held,
            Some(rec) => match &rec.approved_version {
                None => Approval::Held,
                Some(_) if perms::is_subset(requested, &rec.reviewed) => {
                    Approval::Granted(perms::intersection(requested, &rec.granted))
                }
                Some(_) => Approval::Held,
            },
        }
    }

    /// Plugin directories that could hold a plugin: visible sub-directories,
    /// sorted so hook order is deterministic.
    pub(crate) fn candidates(&self) -> Result<Vec<PathBuf>, ManagerError> {
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

    /// Compile `candidate` against the permissions it requests, without
    /// instantiating it or running any of its code.
    pub(crate) fn validate(&self, candidate: &Candidate) -> Result<(), LoadError> {
        self.runtime
            .load_with(&candidate.manifest, &candidate.wasm, self.services.clone())
            .map(drop)?;
        Ok(())
    }

    /// Read, validate and compile one plugin directory without touching the
    /// registry.
    fn prepare(&self, dir: &Path) -> Result<Prep, Box<LoadFailure>> {
        let fail = |plugin: Option<String>, error: LoadError| {
            Box::new(LoadFailure {
                dir: dir.to_path_buf(),
                plugin,
                error,
            })
        };
        let (mut manifest, manifest_bytes) = read_manifest(dir).map_err(|e| fail(None, e))?;
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
        let version = manifest.plugin.version.to_string();

        if self.disabled_by(&name).is_some() {
            return Ok(Prep::Disabled(name));
        }
        self.policy
            .check(&PolicyRequest {
                action: PolicyAction::Enable,
                name: &name,
                version: &version,
                publisher: manifest.plugin.publisher.as_deref(),
                signed: false,
            })
            .map_err(|d| fail(LoadError::Policy(d.reason)))?;

        let (grant, auto_approve) = match self.approval(&name, &manifest) {
            Approval::Held => return Ok(Prep::Held { name, version }),
            Approval::Auto => (manifest.permissions.clone(), true),
            Approval::Granted(grant) => (grant, false),
        };
        // The plugin runs with what was approved, never with more.
        manifest.permissions = grant;

        let wasm = read_entry(dir, &manifest).map_err(&fail)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&(manifest_bytes.len() as u64).to_le_bytes());
        hasher.update(&manifest_bytes);
        hasher.update(&wasm);
        hasher.update(format!("{:?}", manifest.permissions).as_bytes());
        let fingerprint = hasher.finalize();
        if self
            .loaded
            .lock()
            .get(&name)
            .is_some_and(|l| l.fingerprint == fingerprint)
        {
            return Ok(Prep::Unchanged(name));
        }

        let plugin = self
            .runtime
            .load_with(&manifest, &wasm, self.services.clone())
            .map_err(|e| fail(e.into()))?;
        let info = PluginInfo {
            name: name.clone(),
            version,
            description: manifest.plugin.description.clone(),
            dir: dir.to_path_buf(),
            cache: plugin.cache_status().clone(),
        };
        Ok(Prep::Load(Box::new(Prepared {
            manifest,
            plugin,
            info,
            fingerprint,
            auto_approve,
        })))
    }

    /// Register a prepared plugin, replacing a running one of the same name.
    fn commit(&self, prepared: Prepared) -> SyncOutcome {
        let _ops = self.ops.lock();
        let Prepared {
            manifest,
            plugin,
            info,
            fingerprint,
            auto_approve,
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
                    name.clone(),
                    Loaded {
                        info: info.clone(),
                        fingerprint,
                    },
                );
                self.failures.lock().remove(&name);
                self.held.lock().remove(&name);
                self.record_loaded(&info, auto_approve);
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

    /// Note in the registry that `info`'s version is the approved, running one.
    fn record_loaded(&self, info: &PluginInfo, auto_approve: bool) {
        let mut reg = self.registry.lock();
        let changed = if auto_approve {
            let rec = reg.entry(&info.name);
            rec.approved_version = Some(info.version.clone());
            rec.installed_at = now_secs();
            rec.source.get_or_insert_with(|| "folder".to_owned());
            true
        } else if let Some(rec) = reg.get(&info.name)
            && (rec.approved_version.as_deref() != Some(&info.version)
                || rec.pending_version.as_deref() == Some(&info.version))
        {
            let rec = reg.entry(&info.name);
            rec.approved_version = Some(info.version.clone());
            if rec.pending_version.as_deref() == Some(&info.version) {
                rec.pending_version = None;
            }
            true
        } else {
            false
        };
        if changed && let Err(e) = reg.save() {
            drop(reg);
            self.warn(e.to_string());
        }
    }

    fn record_failure(&self, failure: &LoadFailure) {
        let name = failure.display_name();
        self.failures.lock().insert(
            name.clone(),
            FailureRecord {
                name,
                dir: failure.dir.clone(),
                error: failure.error.to_string(),
            },
        );
    }

    /// Unregister `name` if loaded; whether it was.
    pub(crate) fn unload_quiet(&self, name: &str) -> bool {
        let _ops = self.ops.lock();
        if self.loaded.lock().remove(name).is_none() {
            return false;
        }
        // The plugin is gone from the registry already if unregister fails.
        let _ = self.kernel.unregister(name);
        true
    }

    /// Turn what `prepare` found into registry changes and an outcome.
    fn apply(&self, result: Result<Prep, Box<LoadFailure>>) -> SyncOutcome {
        match result {
            Err(f) => {
                self.record_failure(&f);
                SyncOutcome::Failed(*f)
            }
            Ok(Prep::Unchanged(name)) => {
                self.held.lock().remove(&name);
                SyncOutcome::Unchanged(name)
            }
            Ok(Prep::Disabled(name)) => {
                self.failures.lock().remove(&name);
                self.held.lock().remove(&name);
                let unloaded = self.unload_quiet(&name);
                SyncOutcome::Disabled { name, unloaded }
            }
            Ok(Prep::Held { name, version }) => {
                self.failures.lock().remove(&name);
                self.held.lock().insert(
                    name.clone(),
                    Held {
                        version: version.clone(),
                    },
                );
                SyncOutcome::NeedsApproval { name, version }
            }
            Ok(Prep::Load(p)) => {
                let outcome = self.commit(*p);
                if let SyncOutcome::Failed(f) = &outcome {
                    self.record_failure(f);
                }
                outcome
            }
        }
    }

    /// Tell the listener what an outcome means.
    pub(crate) fn notify(&self, outcome: &SyncOutcome) {
        let state = |name: &str| ManagerEvent::StateChanged {
            name: name.to_owned(),
        };
        match outcome {
            SyncOutcome::Loaded(i) => {
                self.emit(ManagerEvent::Loaded {
                    name: i.name.clone(),
                });
                self.emit(state(&i.name));
            }
            SyncOutcome::Reloaded(i) => {
                self.emit(ManagerEvent::Reloaded {
                    name: i.name.clone(),
                });
                self.emit(state(&i.name));
            }
            SyncOutcome::Unloaded(name) => {
                self.emit(ManagerEvent::Unloaded { name: name.clone() });
                self.emit(state(name));
            }
            SyncOutcome::Disabled { name, unloaded } => {
                if *unloaded {
                    self.emit(ManagerEvent::Unloaded { name: name.clone() });
                }
                self.emit(state(name));
            }
            SyncOutcome::NeedsApproval { name, version } => {
                self.emit(ManagerEvent::NeedsApproval {
                    name: name.clone(),
                    version: version.clone(),
                });
                self.emit(state(name));
            }
            SyncOutcome::Failed(f) => {
                let name = f.display_name();
                self.emit(ManagerEvent::Failed {
                    name: name.clone(),
                    error: f.error.to_string(),
                });
                self.emit(state(&name));
            }
            SyncOutcome::Unchanged(_) | SyncOutcome::Absent => {}
        }
    }

    /// Load every plugin directory, compiling in parallel. A broken plugin is
    /// reported in [`ScanReport::failed`] (and kept for
    /// [`PluginManager::list`]) and does not stop the rest. Already running
    /// plugins whose files changed are reloaded; disabled ones stay off and
    /// ones needing approval stay unloaded.
    pub fn scan(&self) -> Result<ScanReport, ManagerError> {
        let dirs = self.candidates()?;
        let prepared: Vec<_> = dirs.par_iter().map(|dir| self.prepare(dir)).collect();
        let mut report = ScanReport::default();
        for result in prepared {
            let outcome = self.apply(result);
            self.notify(&outcome);
            match outcome {
                SyncOutcome::Loaded(i) | SyncOutcome::Reloaded(i) => report.loaded.push(i),
                SyncOutcome::NeedsApproval { name, .. } | SyncOutcome::Disabled { name, .. } => {
                    report.held.push(name);
                }
                SyncOutcome::Failed(f) => report.failed.push(f),
                SyncOutcome::Unchanged(_) | SyncOutcome::Unloaded(_) | SyncOutcome::Absent => {}
            }
        }
        // Plugins that vanished while the app was away have nothing to scan.
        self.forget_missing();
        self.emit(ManagerEvent::Scanned {
            loaded: report.loaded.len(),
            failed: report.failed.len(),
        });
        Ok(report)
    }

    /// Drop failure and hold records of directories that no longer exist.
    fn forget_missing(&self) {
        let gone = |name: &String| !self.root.join(name).is_dir();
        self.failures.lock().retain(|name, _| !gone(name));
        self.held.lock().retain(|name, _| !gone(name));
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
        let outcome = if !dir.is_dir() {
            self.failures.lock().remove(name);
            self.held.lock().remove(name);
            if self.unload_quiet(name) {
                SyncOutcome::Unloaded(name.to_owned())
            } else {
                SyncOutcome::Absent
            }
        } else {
            let prepared = self.prepare(&dir);
            self.apply(prepared)
        };
        self.notify(&outcome);
        outcome
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
        if self.unload_quiet(name) {
            self.notify(&SyncOutcome::Unloaded(name.to_owned()));
            Ok(())
        } else {
            Err(ManagerError::NotLoaded(name.to_owned()))
        }
    }

    /// Put built-in plugins in the state the registry and the open project
    /// ask for: switched-off ones are taken out of the kernel (and kept to be
    /// put back), switched-on ones restored. Core plugins are never touched.
    /// Returns the names whose state changed.
    pub fn reconcile_builtins(&self) -> Vec<String> {
        let _ops = self.ops.lock();
        let mut changed = Vec::new();
        let mut names: BTreeSet<String> = self.kernel.plugin_names().into_iter().collect();
        names.extend(self.natives_off.lock().keys().cloned());
        for name in names {
            if self.core.contains(&name) || self.loaded.lock().contains_key(&name) {
                continue;
            }
            let want_on = self.disabled_by(&name).is_none();
            let registered = self.kernel.manifest(&name).is_some();
            if registered && !want_on {
                let Some((manifest, plugin)) = self.kernel.plugin_handle(&name) else {
                    continue;
                };
                if manifest.plugin.tier != Tier::Native {
                    continue;
                }
                if self.kernel.unregister(&name).is_ok() {
                    self.natives_off
                        .lock()
                        .insert(name.clone(), (manifest, plugin));
                    changed.push(name);
                }
            } else if !registered
                && want_on
                && let Some((manifest, plugin)) = self.natives_off.lock().remove(&name)
            {
                match self.kernel.register(manifest.clone(), Arc::clone(&plugin)) {
                    Ok(()) => changed.push(name),
                    Err(e) => {
                        self.natives_off
                            .lock()
                            .insert(name.clone(), (manifest, plugin));
                        self.warn(format!("cannot re-enable `{name}`: {e}"));
                    }
                }
            }
        }
        changed
    }

    /// Apply the open project's override (`<project>/.manang/plugins.toml`),
    /// or clear it with `None`. A project can only disable plugins. Returns
    /// the names that changed state; a core plugin the file lists is ignored.
    pub fn set_project(&self, project: Option<&Path>) -> Result<Vec<String>, ManagerError> {
        let next = match project {
            Some(path) => ProjectOverride::load(path)?,
            None => ProjectOverride::default(),
        };
        let _admin = self.admin.lock();
        let previous = std::mem::replace(&mut *self.project.lock(), next.clone());
        let mut names: BTreeSet<String> = previous.disabled.into_iter().collect();
        names.extend(next.disabled);
        let mut changed = self.reconcile_builtins();
        for name in names {
            if self.core.contains(&name) {
                continue;
            }
            if self.root.join(&name).is_dir() {
                self.sync(&name);
                changed.push(name);
            }
        }
        for name in &changed {
            self.emit(ManagerEvent::StateChanged { name: name.clone() });
        }
        changed.sort();
        changed.dedup();
        Ok(changed)
    }
}
