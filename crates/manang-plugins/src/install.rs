//! Lifecycle operations: inspect, install, approve, enable, disable,
//! uninstall. Nothing here runs plugin code: validation compiles the
//! component and checks its imports against the manifest, never instantiates.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use manang_kernel::Tier;
use manang_kernel::manifest::Permissions;
use serde::Serialize;

use crate::archive::{self, ARCHIVE_EXTENSION, ArchiveError, extract_archive, is_allowed};
use crate::error::{LoadError, ManagerError};
use crate::events::ManagerEvent;
use crate::manager::{
    Candidate, DEFAULT_ENTRY, PluginManager, now_secs, read_entry, read_manifest,
};
use crate::perms;
use crate::policy::{PolicyAction, PolicyRequest};
use crate::registry::valid_plugin_name;
use crate::state::{CommandRef, PluginState};

static STAGING: AtomicU64 = AtomicU64::new(0);

/// A scratch directory inside the plugins directory (same file system, so the
/// final rename is atomic), removed when dropped unless [`Staging::keep`] ran.
struct Staging {
    path: PathBuf,
    keep: bool,
}

impl Staging {
    fn create(root: &Path) -> Result<Self, ManagerError> {
        let n = STAGING.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(".staging-{}-{n}", std::process::id()));
        fs::create_dir(&path).map_err(|source| ManagerError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(Self { path, keep: false })
    }

    /// The directory now belongs to the caller (it has been renamed away).
    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.keep {
            // Our own scratch directory; nothing of the user's is in it.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// What a plugin archive or folder contains, for the install dialog. Reading
/// it installs nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectReport {
    pub name: String,
    pub version: String,
    pub description: String,
    pub tier: Tier,
    pub publisher: Option<String>,
    pub signed: bool,
    /// `archive` or `folder`.
    pub kind: &'static str,
    pub permissions: Permissions,
    /// What it asks for beyond what the user already approved for this plugin
    /// (everything, for a new plugin).
    pub new_permissions: Permissions,
    pub commands: Vec<CommandRef>,
    /// The version installed now, when this would update it.
    pub installed_version: Option<String>,
}

/// What `plugins.uninstall` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UninstallReport {
    pub name: String,
    /// Where the files went. They are never deleted.
    pub trashed_to: Option<String>,
}

fn check_name(name: &str) -> Result<(), ManagerError> {
    if valid_plugin_name(name) {
        Ok(())
    } else {
        Err(ManagerError::InvalidName(name.chars().take(80).collect()))
    }
}

fn invalid(e: impl Into<LoadError>) -> ManagerError {
    ManagerError::Invalid(Box::new(e.into()))
}

struct Staged {
    staging: Staging,
    candidate: Candidate,
    kind: &'static str,
}

impl PluginManager {
    /// Unpack or copy `path` into a staging directory and validate it.
    fn stage(&self, path: &Path) -> Result<Staged, ManagerError> {
        let meta = fs::metadata(path)
            .map_err(|_| ManagerError::InvalidPath(path.display().to_string()))?;
        let staging = Staging::create(&self.root)?;
        let (kind, root_name) = if meta.is_dir() {
            archive::copy_folder(path, &staging.path)?;
            (
                "folder",
                path.file_name().map(|n| n.to_string_lossy().into_owned()),
            )
        } else {
            let ok = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ARCHIVE_EXTENSION));
            if !ok {
                return Err(ManagerError::InvalidPath(path.display().to_string()));
            }
            ("archive", extract_archive(path, &staging.path)?)
        };
        let (manifest, _) = read_manifest(&staging.path).map_err(invalid)?;
        // A folder (or the folder inside an archive) must be named after the
        // plugin, as an installed plugin's folder is.
        if let Some(dir) = root_name
            && dir != manifest.plugin.name
        {
            return Err(invalid(LoadError::NameMismatch {
                dir,
                name: manifest.plugin.name.clone(),
            }));
        }
        let entry = manifest.plugin.entry.as_deref().unwrap_or(DEFAULT_ENTRY);
        if !is_allowed(entry) {
            return Err(ManagerError::Archive(ArchiveError::Unexpected(
                entry.to_owned(),
            )));
        }
        let wasm = read_entry(&staging.path, &manifest).map_err(invalid)?;
        let candidate = Candidate { manifest, wasm };
        // Compile and check imports against the requested permissions.
        self.validate(&candidate).map_err(invalid)?;
        Ok(Staged {
            staging,
            candidate,
            kind,
        })
    }

    fn policy_check(
        &self,
        action: PolicyAction,
        name: &str,
        version: &str,
        publisher: Option<&str>,
    ) -> Result<(), ManagerError> {
        self.policy
            .check(&PolicyRequest {
                action,
                name,
                version,
                publisher,
                signed: false,
            })
            .map_err(|d| ManagerError::Denied {
                name: name.to_owned(),
                reason: d.reason,
            })
    }

    /// Read a plugin archive or folder and report what it is and what it
    /// asks for, without installing it.
    pub fn inspect(&self, path: &Path) -> Result<InspectReport, ManagerError> {
        let staged = self.stage(path)?;
        let m = &staged.candidate.manifest;
        let record = self.registry.lock().get(&m.plugin.name).cloned();
        let new_permissions = match &record {
            Some(r) if r.approved_version.is_some() => {
                perms::difference(&m.permissions, &r.reviewed)
            }
            _ => m.permissions.clone(),
        };
        let installed_version = read_manifest(&self.root.join(&m.plugin.name))
            .ok()
            .map(|(old, _)| old.plugin.version.to_string());
        Ok(InspectReport {
            name: m.plugin.name.clone(),
            version: m.plugin.version.to_string(),
            description: m.plugin.description.clone(),
            tier: m.plugin.tier,
            publisher: m.plugin.publisher.clone(),
            signed: false,
            kind: staged.kind,
            permissions: m.permissions.clone(),
            new_permissions,
            commands: m
                .contributes
                .commands
                .iter()
                .map(|c| CommandRef {
                    id: c.id.clone(),
                    title: c.title.clone(),
                })
                .collect(),
            installed_version,
        })
    }

    /// Move `path` into the trash folder: `<trash>/<seconds>-<n>/<label>`.
    /// Nothing is ever deleted.
    fn trash_path(&self, path: &Path, label: &str) -> Result<PathBuf, ManagerError> {
        let fail = |source| ManagerError::Trash {
            from: path.to_path_buf(),
            source,
        };
        fs::create_dir_all(&self.trash).map_err(fail)?;
        let ts = now_secs();
        for n in 0u32.. {
            let slot = self.trash.join(format!("{ts}-{n}"));
            match fs::create_dir(&slot) {
                Ok(()) => {
                    let dest = slot.join(label);
                    fs::rename(path, &dest).map_err(fail)?;
                    return Ok(dest);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(fail(e)),
            }
        }
        Err(fail(std::io::Error::other("no free trash slot")))
    }

    fn save_registry(&self) -> Result<(), ManagerError> {
        self.registry.lock().save()?;
        Ok(())
    }

    /// Record that `name` came from the plugin registry.
    pub(crate) fn set_origin(
        &self,
        name: &str,
        origin: crate::registry::Origin,
    ) -> Result<(), ManagerError> {
        {
            let mut reg = self.registry.lock();
            let rec = reg.entry(name);
            rec.source = Some("registry".to_owned());
            rec.origin = Some(origin);
        }
        self.save_registry()
    }

    /// Install a plugin from a folder or a `.manangplugin` archive. The files
    /// are validated, copied into place atomically and recorded as waiting for
    /// approval: nothing of the plugin is loaded or run. A newer version of an
    /// installed plugin that asks for more than was approved waits beside the
    /// running one, which keeps running.
    pub fn install(&self, path: &Path) -> Result<PluginState, ManagerError> {
        let _admin = self.admin.lock();
        let mut staged = self.stage(path)?;
        let m = staged.candidate.manifest.clone();
        let name = m.plugin.name.clone();
        let version = m.plugin.version.to_string();
        self.policy_check(
            PolicyAction::Install,
            &name,
            &version,
            m.plugin.publisher.as_deref(),
        )?;
        if self.core.contains(&name) || self.is_builtin(&name) {
            return Err(ManagerError::BuiltIn(name));
        }
        let dest = self.root.join(&name);
        let record = self.registry.lock().get(&name).cloned();
        let source = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let installed = if dest.is_dir() {
            Some(read_manifest(&dest).ok().map(|(old, _)| old.plugin.version))
        } else {
            None
        };

        match (installed, record) {
            (None, _) => {
                // A fresh install: record first, so a racing watcher sees the
                // plugin as awaiting approval, then move the files in.
                {
                    let mut reg = self.registry.lock();
                    let rec = reg.entry(&name);
                    *rec = crate::registry::PluginRecord {
                        installed_at: now_secs(),
                        source: Some(source),
                        publisher: m.plugin.publisher.clone(),
                        ..Default::default()
                    };
                }
                self.save_registry()?;
                if let Err(source) = fs::rename(&staged.staging.path, &dest) {
                    self.registry.lock().remove(&name);
                    self.save_registry()?;
                    return Err(ManagerError::Io { path: dest, source });
                }
                staged.staging.keep();
            }
            (Some(_), None) => {
                return Err(ManagerError::AlreadyInstalled {
                    name,
                    version: "unmanaged".to_owned(),
                });
            }
            (Some(current), Some(rec)) => {
                if rec.approved_version.is_some()
                    && let Some(current) = &current
                    && m.plugin.version <= *current
                {
                    return Err(ManagerError::AlreadyInstalled {
                        name,
                        version: current.to_string(),
                    });
                }
                let within_reviewed = rec.approved_version.is_some()
                    && perms::is_subset(&m.permissions, &rec.reviewed);
                if rec.approved_version.is_none() || within_reviewed {
                    // Replace in place: nothing new to approve.
                    self.trash_path(&dest, &format!("{name}-previous"))?;
                    fs::rename(&staged.staging.path, &dest).map_err(|source| ManagerError::Io {
                        path: dest.clone(),
                        source,
                    })?;
                    staged.staging.keep();
                    {
                        let mut reg = self.registry.lock();
                        let rec = reg.entry(&name);
                        rec.pending_version = None;
                        rec.source = Some(source);
                    }
                    self.save_registry()?;
                } else {
                    // Asks for more: park it, keep the old one running.
                    let pending = self.root.join(format!(".pending-{name}"));
                    if pending.exists() {
                        self.trash_path(&pending, &format!("{name}-pending"))?;
                    }
                    fs::rename(&staged.staging.path, &pending).map_err(|source| {
                        ManagerError::Io {
                            path: pending.clone(),
                            source,
                        }
                    })?;
                    staged.staging.keep();
                    {
                        let mut reg = self.registry.lock();
                        let rec = reg.entry(&name);
                        rec.pending_version = Some(version.clone());
                        rec.source = Some(source);
                    }
                    self.save_registry()?;
                    self.held.lock().insert(
                        name.clone(),
                        crate::manager::Held {
                            version: version.clone(),
                        },
                    );
                    self.emit(ManagerEvent::NeedsApproval {
                        name: name.clone(),
                        version,
                    });
                    self.emit(ManagerEvent::StateChanged { name: name.clone() });
                    return self.state_of(&name);
                }
            }
        }
        self.sync(&name);
        self.state_of(&name)
    }

    /// Approve what an installed plugin asked for and load it. `granted` must
    /// be the requested set or a subset of it. For an update that was waiting,
    /// the new version replaces the old one.
    pub fn approve(&self, name: &str, granted: &Permissions) -> Result<PluginState, ManagerError> {
        check_name(name)?;
        let _admin = self.admin.lock();
        let dir = self.root.join(name);
        let pending_dir = self.root.join(format!(".pending-{name}"));
        let record = self.registry.lock().get(name).cloned();
        let use_pending =
            pending_dir.is_dir() && record.as_ref().is_some_and(|r| r.pending_version.is_some());
        let source_dir = if use_pending { &pending_dir } else { &dir };
        if !source_dir.is_dir() {
            return Err(ManagerError::NotInstalled(name.to_owned()));
        }
        let (manifest, _) = read_manifest(source_dir).map_err(invalid)?;
        let waiting = use_pending
            || self.held.lock().contains_key(name)
            || record.as_ref().is_none_or(|r| r.approved_version.is_none());
        if !waiting {
            return Err(ManagerError::NothingToApprove(name.to_owned()));
        }
        let requested = manifest.permissions.clone();
        if !perms::is_subset(granted, &requested) {
            let extra = perms::difference(granted, &requested);
            return Err(ManagerError::PermissionMismatch(format!("{extra:?}")));
        }
        self.policy_check(
            PolicyAction::Enable,
            name,
            &manifest.plugin.version.to_string(),
            manifest.plugin.publisher.as_deref(),
        )?;
        if use_pending {
            self.trash_path(&dir, &format!("{name}-previous"))?;
            fs::rename(&pending_dir, &dir).map_err(|source| ManagerError::Io {
                path: dir.clone(),
                source,
            })?;
        }
        {
            let mut reg = self.registry.lock();
            let rec = reg.entry(name);
            if rec.installed_at == 0 {
                rec.installed_at = now_secs();
                rec.source.get_or_insert_with(|| "folder".to_owned());
            }
            rec.granted = perms::intersection(granted, &requested);
            rec.reviewed = requested;
            rec.approved_version = Some(manifest.plugin.version.to_string());
            rec.pending_version = None;
        }
        self.save_registry()?;
        self.held.lock().remove(name);
        self.sync(name);
        self.state_of(name)
    }

    fn is_builtin(&self, name: &str) -> bool {
        !self.loaded.lock().contains_key(name)
            && !self.root.join(name).is_dir()
            && (self.kernel.manifest(name).is_some() || self.natives_off.lock().contains_key(name))
    }

    /// Turn a plugin on.
    pub fn enable(&self, name: &str) -> Result<PluginState, ManagerError> {
        self.set_enabled(name, true)
    }

    /// Turn a plugin off: it is unloaded but stays installed.
    pub fn disable(&self, name: &str) -> Result<PluginState, ManagerError> {
        self.set_enabled(name, false)
    }

    fn set_enabled(&self, name: &str, on: bool) -> Result<PluginState, ManagerError> {
        check_name(name)?;
        let _admin = self.admin.lock();
        if self.core.contains(name) {
            return Err(ManagerError::Core(name.to_owned()));
        }
        if self.is_builtin(name) {
            if on {
                self.registry.lock().remove(name);
            } else {
                self.registry.lock().entry(name).enabled = false;
            }
            self.save_registry()?;
            self.reconcile_builtins();
            self.emit(ManagerEvent::StateChanged {
                name: name.to_owned(),
            });
            return self.state_of(name);
        }
        let dir = self.root.join(name);
        if !dir.is_dir() {
            return Err(ManagerError::NotInstalled(name.to_owned()));
        }
        if on {
            let (manifest, _) = read_manifest(&dir).map_err(invalid)?;
            self.policy_check(
                PolicyAction::Enable,
                name,
                &manifest.plugin.version.to_string(),
                manifest.plugin.publisher.as_deref(),
            )?;
        }
        self.registry.lock().entry(name).enabled = on;
        self.save_registry()?;
        self.sync(name);
        self.state_of(name)
    }

    /// Re-read a plugin from disk and try loading it again (also how a failed
    /// plugin is retried).
    pub fn reload_plugin(&self, name: &str) -> Result<PluginState, ManagerError> {
        check_name(name)?;
        let _admin = self.admin.lock();
        if self.is_builtin(name) {
            return Err(ManagerError::BuiltIn(name.to_owned()));
        }
        if !self.root.join(name).is_dir() {
            return Err(ManagerError::NotInstalled(name.to_owned()));
        }
        self.sync(name);
        self.state_of(name)
    }

    /// Unload a plugin and move its files to the trash folder. Built-in
    /// plugins cannot be uninstalled.
    pub fn uninstall(&self, name: &str) -> Result<UninstallReport, ManagerError> {
        check_name(name)?;
        let _admin = self.admin.lock();
        if self.core.contains(name) {
            return Err(ManagerError::Core(name.to_owned()));
        }
        if self.is_builtin(name) {
            return Err(ManagerError::BuiltIn(name.to_owned()));
        }
        let dir = self.root.join(name);
        let pending = self.root.join(format!(".pending-{name}"));
        let known = self.registry.lock().get(name).is_some();
        if !dir.is_dir() && !known {
            return Err(ManagerError::NotInstalled(name.to_owned()));
        }
        let was_loaded = self.unload_quiet(name);
        let trashed = if dir.is_dir() {
            Some(self.trash_path(&dir, name)?)
        } else {
            None
        };
        if pending.is_dir() {
            self.trash_path(&pending, &format!("{name}-pending"))?;
        }
        self.registry.lock().remove(name);
        self.save_registry()?;
        self.failures.lock().remove(name);
        self.held.lock().remove(name);
        if was_loaded {
            self.emit(ManagerEvent::Unloaded {
                name: name.to_owned(),
            });
        }
        self.emit(ManagerEvent::StateChanged {
            name: name.to_owned(),
        });
        Ok(UninstallReport {
            name: name.to_owned(),
            trashed_to: trashed.map(|p| p.display().to_string()),
        })
    }
}
