//! The plugin marketplace on the app side: browse the signed registry index,
//! read a plugin's page, install or update from it, and check for updates.
//!
//! Nothing here installs by itself. An install downloads the archive, checks
//! its size and SHA-256 against the verified index, reads the archive's own
//! manifest and refuses it unless that manifest asks for exactly the
//! permissions the index showed the person, and only then hands it to the same
//! stage, consent and sandbox path a hand-picked `.manangplugin` takes
//! ([`PluginManager::install`] and [`PluginManager::approve`]). Updates are
//! looked for, never applied: every update needs its own consent.
//!
//! FOLLOW-UP (not built, see ADR 0016): there is no revocation list. A takedown
//! (a version leaving the index or listed as `yanked`) stops new installs and
//! updates only; it does not disable an installed copy. A signed revocation
//! list is the planned fix.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use manang_kernel::manifest::Permissions;
use manang_registry as reg;
use manang_registry::{
    Base, Cache, Freshness, Listing, PluginEntry, Query, Registry, RegistryError, Sort, Transport,
    TrustedKeys,
};
use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::error::ManagerError;
use crate::events::ManagerEvent;
use crate::manager::{PluginManager, now_secs};
use crate::perms;
use crate::policy::{PolicyAction, PolicyRequest};
use crate::registry::Origin;

/// Background update checks ask the network at most this often.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
const README_LIMIT: u64 = 256 * 1024;
const PROGRESS_STEP: Duration = Duration::from_millis(100);

static DOWNLOADS: AtomicU64 = AtomicU64::new(0);

/// How the marketplace reaches the registry. The defaults are the build's
/// registry address, the embedded keys and the real HTTP client.
#[derive(Clone)]
pub struct MarketplaceConfig {
    /// Where the verified index is cached. `None` keeps nothing on disk.
    pub cache_dir: Option<PathBuf>,
    /// The build's registry address (`MANANG_REGISTRY_URL`).
    pub registry_url: String,
    pub keys: TrustedKeys,
    pub transport: Arc<dyn Transport>,
    /// This build's version, for `min_app` checks.
    pub app_version: String,
}

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            cache_dir: None,
            registry_url: reg::DEFAULT_URL.to_owned(),
            keys: TrustedKeys::embedded(),
            transport: Arc::new(reg::HttpTransport),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

struct Settings {
    /// The user's override; empty means the build's address.
    url: String,
    browse: bool,
}

pub struct Marketplace {
    config: MarketplaceConfig,
    settings: Mutex<Settings>,
    busy: AtomicBool,
    cancel: AtomicBool,
}

/// The `[plugin].permissions` of a registry entry as the manifest type.
fn kernel_permissions(p: &reg::Permissions) -> Permissions {
    Permissions {
        filesystem: p.filesystem.clone(),
        network: p.network.clone(),
        buffer: p.buffer.clone(),
    }
}

fn same_permissions(a: &Permissions, b: &Permissions) -> bool {
    perms::is_subset(a, b) && perms::is_subset(b, a)
}

fn permissions_json(p: &Permissions) -> Value {
    json!({ "filesystem": p.filesystem, "network": p.network, "buffer": p.buffer })
}

/// A scratch directory for one download, removed when dropped.
struct DownloadDir(PathBuf);

impl Drop for DownloadDir {
    fn drop(&mut self) {
        // Our own scratch directory.
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Installed {
    /// What the update check compares: the waiting version if one is parked,
    /// otherwise the running one.
    version: String,
}

impl Marketplace {
    pub fn new(config: MarketplaceConfig) -> Self {
        Self {
            config,
            settings: Mutex::new(Settings {
                url: String::new(),
                browse: true,
            }),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        }
    }

    // ------------------------------------------------------------ settings

    /// `plugins.configure {values}`: `plugins.browse` (bool) and
    /// `plugins.registry-url` (string, empty for the default). All or nothing.
    pub fn configure(&self, args: &Value) -> Result<Value, ManagerError> {
        let values = args.get("values").and_then(Value::as_object);
        let mut next = {
            let s = self.settings.lock();
            Settings {
                url: s.url.clone(),
                browse: s.browse,
            }
        };
        for (key, value) in values.into_iter().flatten() {
            match (key.as_str(), value) {
                ("plugins.browse", Value::Bool(b)) => next.browse = *b,
                ("plugins.registry-url", Value::String(u)) => {
                    let u = u.trim();
                    if !u.is_empty() && matches!(Base::parse(u), Err(RegistryError::InsecureUrl)) {
                        return Err(RegistryError::InsecureUrl.into());
                    }
                    next.url = u.to_owned();
                }
                _ => {}
            }
        }
        let out = json!({
            "plugins.browse": next.browse,
            "plugins.registry-url": next.url,
        });
        *self.settings.lock() = next;
        Ok(out)
    }

    fn url(&self) -> String {
        let s = self.settings.lock();
        if s.url.is_empty() {
            self.config.registry_url.clone()
        } else {
            s.url.clone()
        }
    }

    fn registry(&self) -> Result<Registry<Arc<dyn Transport>>, ManagerError> {
        let base = Base::parse(&self.url())?;
        Ok(Registry::new(
            Arc::clone(&self.config.transport),
            base,
            self.config.keys.clone(),
            Cache::new(self.config.cache_dir.clone()),
        ))
    }

    /// Browse is on (the setting) and policy allows it.
    fn gate(&self, manager: &PluginManager) -> Result<(), ManagerError> {
        if !self.settings.lock().browse {
            return Err(ManagerError::BrowseOff(
                " (Settings, Plugins, Browse the plugin registry)".to_owned(),
            ));
        }
        manager
            .policy
            .check(&PolicyRequest {
                action: PolicyAction::Browse,
                name: "registry",
                version: "",
                publisher: None,
                signed: false,
            })
            .map_err(|d| ManagerError::BrowseOff(format!(": {}", d.reason)))
    }

    // ------------------------------------------------------------ helpers

    fn listing(&self, refresh: bool) -> Result<(Listing, bool), ManagerError> {
        let registry = self.registry()?;
        if !refresh && let Some(cached) = registry.cached() {
            return Ok((cached, true));
        }
        Ok((registry.refresh(now_secs())?, false))
    }

    /// Installed registry-eligible plugins: id to what to compare.
    fn installed(manager: &PluginManager) -> BTreeMap<String, Installed> {
        manager
            .list()
            .into_iter()
            .filter(|s| s.tier == manang_kernel::Tier::Wasm)
            .map(|s| {
                let version = if s.update_pending {
                    s.version.clone()
                } else {
                    s.running_version
                        .clone()
                        .unwrap_or_else(|| s.version.clone())
                };
                (s.name, Installed { version })
            })
            .collect()
    }

    fn freshness_json(listing: &Listing, from_cache: bool) -> Value {
        let (state, reason) = match &listing.freshness {
            _ if from_cache => ("cached", None),
            Freshness::Fresh => ("fresh", None),
            Freshness::Unchanged => ("unchanged", None),
            Freshness::Offline { reason } => ("offline", Some(reason.clone())),
        };
        json!({
            "state": state,
            "reason": reason,
            "fetched_at": listing.fetched_at,
            "generated": listing.index.generated,
        })
    }

    fn summary(
        &self,
        p: &PluginEntry,
        listing: &Listing,
        installed: &BTreeMap<String, Installed>,
    ) -> Value {
        let latest = p.latest_version();
        let mine = installed.get(&p.id);
        json!({
            "id": p.id,
            "name": p.name,
            "author": p.author,
            "description": p.description,
            "tags": p.tags,
            "license": p.license,
            "latest": p.latest,
            "published": latest.map(|v| v.published.clone()),
            "size": latest.map(|v| v.size),
            "downloads": listing.stats.downloads.get(&p.id),
            "compatible": latest.is_none_or(|v| self.compatible(v)),
            "installed": mine.map(|i| json!({
                "version": i.version,
                "update_available": reg::is_newer(&p.latest, &i.version)
                    && !p.is_yanked(&p.latest),
            })),
        })
    }

    fn compatible(&self, v: &reg::VersionEntry) -> bool {
        v.min_app
            .as_deref()
            .is_none_or(|min| reg::app_satisfies(&self.config.app_version, min))
    }

    // ------------------------------------------------------------ commands

    /// `plugins.registry-status`: what the marketplace is set up to do. No
    /// network.
    pub fn status(&self, manager: &PluginManager) -> Value {
        let browse = self.settings.lock().browse;
        let gate = self.gate(manager);
        let base = Base::parse(&self.url());
        let configured = base.is_ok() && !self.config.keys.is_empty();
        let reason = match (&gate, &base) {
            (Err(e), _) => Some(e.to_string()),
            (_, Err(e)) => Some(e.to_string()),
            _ if self.config.keys.is_empty() => Some(RegistryError::NotConfigured.to_string()),
            _ => None,
        };
        let cached = if gate.is_ok() && configured {
            self.registry().ok().and_then(|r| r.cached())
        } else {
            None
        };
        let updates = cached.as_ref().map_or(0, |l| {
            let installed: Vec<(String, String)> = Self::installed(manager)
                .into_iter()
                .map(|(id, i)| (id, i.version))
                .collect();
            reg::updates(&l.index, &installed).len()
        });
        json!({
            "enabled": gate.is_ok(),
            "browse_setting": browse,
            "configured": configured,
            "reason": reason,
            "url": self.url(),
            "host": base.ok().map(|b| b.host().to_owned()),
            "custom_url": !self.settings.lock().url.is_empty(),
            "fetched_at": cached.as_ref().map(|l| l.fetched_at),
            "plugin_count": cached.as_ref().map(|l| l.index.plugins.len()),
            "updates": updates,
        })
    }

    /// `plugins.browse {text?, tag?, sort?, refresh?}`.
    pub fn browse(&self, manager: &PluginManager, args: &Value) -> Result<Value, ManagerError> {
        self.gate(manager)?;
        let refresh = args
            .get("refresh")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let query = Query {
            text: args
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            tag: args
                .get("tag")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_owned),
            sort: args
                .get("sort")
                .and_then(Value::as_str)
                .and_then(Sort::parse)
                .unwrap_or_default(),
        };
        let (listing, from_cache) = self.listing(refresh)?;
        let installed = Self::installed(manager);
        let found = reg::search(&listing.index, &listing.stats, &query);
        let plugins: Vec<Value> = found
            .iter()
            .map(|p| self.summary(p, &listing, &installed))
            .collect();
        let tags: Vec<Value> = reg::tags(&listing.index)
            .into_iter()
            .map(|(tag, count)| json!({ "tag": tag, "count": count }))
            .collect();
        let pairs: Vec<(String, String)> = installed
            .iter()
            .map(|(id, i)| (id.clone(), i.version.clone()))
            .collect();
        Ok(json!({
            "freshness": Self::freshness_json(&listing, from_cache),
            "total": listing.index.plugins.len(),
            "plugins": plugins,
            "tags": tags,
            "updates": reg::updates(&listing.index, &pairs).len(),
        }))
    }

    /// `plugins.detail {id, readme?}`: one plugin's page from the index, plus
    /// its readme from the registry when it can be fetched.
    pub fn detail(&self, manager: &PluginManager, args: &Value) -> Result<Value, ManagerError> {
        self.gate(manager)?;
        let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
        let (listing, from_cache) = self.listing(false)?;
        let entry = listing
            .index
            .plugin(id)
            .ok_or_else(|| RegistryError::UnknownPlugin(id.chars().take(64).collect()))?;
        let installed = Self::installed(manager);
        let mut readme = Value::Null;
        let mut readme_error = Value::Null;
        if args.get("readme").and_then(Value::as_bool).unwrap_or(true)
            && let Some(path) = &entry.readme
        {
            match self.registry()?.fetch_text(path, README_LIMIT) {
                Ok(text) => readme = Value::String(text),
                Err(e) => readme_error = Value::String(e.to_string()),
            }
        }
        let versions: Vec<Value> = entry
            .versions
            .iter()
            .map(|v| {
                json!({
                    "version": v.version,
                    "published": v.published,
                    "size": v.size,
                    "min_app": v.min_app,
                    "changelog": v.changelog,
                    "permissions": v.permissions,
                    "compatible": self.compatible(v),
                    "yanked": entry.is_yanked(&v.version),
                })
            })
            .collect();
        let latest = entry.latest_version();
        Ok(json!({
            "freshness": Self::freshness_json(&listing, from_cache),
            "plugin": self.summary(entry, &listing, &installed),
            "repo": entry.repo,
            "homepage": entry.homepage,
            "tier": entry.tier,
            "permissions": latest.map(|v| &v.permissions),
            "versions": versions,
            "readme": readme,
            "readme_error": readme_error,
        }))
    }

    /// `plugins.check-updates {background?}`: look for newer versions of the
    /// installed plugins. A background check within a day of the last contact
    /// answers from the cache without touching the network. Never installs.
    pub fn check_updates(
        &self,
        manager: &PluginManager,
        args: &Value,
    ) -> Result<Value, ManagerError> {
        if self.gate(manager).is_err() {
            return Ok(json!({ "enabled": false, "updates": [] }));
        }
        let background = args
            .get("background")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let now = now_secs();
        let registry = self.registry()?;
        let recent = registry
            .cached()
            .filter(|l| now.saturating_sub(l.fetched_at) < CHECK_INTERVAL_SECS);
        let (listing, from_cache) = match recent {
            Some(l) if background => (l, true),
            _ => (registry.refresh(now)?, false),
        };
        let installed = Self::installed(manager);
        let pairs: Vec<(String, String)> = installed
            .iter()
            .map(|(id, i)| (id.clone(), i.version.clone()))
            .collect();
        let found = reg::updates(&listing.index, &pairs);
        let updates: Vec<Value> = found
            .iter()
            .filter_map(|u| {
                let entry = listing.index.plugin(&u.id)?;
                let version = entry.latest_version()?;
                let wanted = kernel_permissions(&version.permissions);
                let reviewed = manager
                    .registry
                    .lock()
                    .get(&u.id)
                    .map(|r| r.reviewed.clone())
                    .unwrap_or_default();
                let added = perms::difference(&wanted, &reviewed);
                Some(json!({
                    "id": u.id,
                    "name": entry.name,
                    "installed": u.installed,
                    "latest": u.latest,
                    "size": version.size,
                    "changelog": version.changelog,
                    "compatible": self.compatible(version),
                    "permissions_grew": !perms::is_empty(&added),
                    "new_permissions": permissions_json(&added),
                }))
            })
            .collect();
        manager.emit(ManagerEvent::UpdatesAvailable {
            count: updates.len(),
        });
        Ok(json!({
            "enabled": true,
            "freshness": Self::freshness_json(&listing, from_cache),
            "updates": updates,
        }))
    }

    /// `plugins.install-from-registry {id, version?, consent}`.
    pub fn install(&self, manager: &PluginManager, args: &Value) -> Result<Value, ManagerError> {
        if args.get("consent").and_then(Value::as_bool) != Some(true) {
            return Err(ManagerError::ConsentRequired);
        }
        self.gate(manager)?;
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err(ManagerError::Busy);
        }
        self.cancel.store(false, Ordering::Release);
        let result = self.install_inner(manager, args);
        self.busy.store(false, Ordering::Release);
        result
    }

    /// `plugins.cancel-install`: stop a running registry download.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    fn install_inner(&self, manager: &PluginManager, args: &Value) -> Result<Value, ManagerError> {
        let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
        let registry = self.registry()?;
        // A fresh look at the list (offline falls back to the cached one; the
        // download then reports the network problem).
        let listing = registry.refresh(now_secs())?;
        let entry = listing
            .index
            .plugin(id)
            .ok_or_else(|| RegistryError::UnknownPlugin(id.chars().take(64).collect()))?;
        let wanted = args
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or(&entry.latest);
        let version = entry
            .version(wanted)
            .ok_or_else(|| RegistryError::UnknownVersion {
                id: id.to_owned(),
                version: wanted.chars().take(40).collect(),
            })?;
        if entry.is_yanked(&version.version) {
            return Err(RegistryError::Withdrawn {
                id: id.to_owned(),
                version: version.version.clone(),
            }
            .into());
        }
        if !self.compatible(version) {
            return Err(RegistryError::NeedsNewerApp {
                id: id.to_owned(),
                version: version.version.clone(),
                needs: version.min_app.clone().unwrap_or_default(),
            }
            .into());
        }

        let n = DOWNLOADS.fetch_add(1, Ordering::Relaxed);
        let dir = DownloadDir(
            manager
                .root
                .join(format!(".download-{}-{n}", std::process::id())),
        );
        fs::create_dir_all(&dir.0).map_err(|source| ManagerError::Io {
            path: dir.0.clone(),
            source,
        })?;
        let file = dir.0.join(format!(
            "{id}-{}.{}",
            version.version,
            crate::archive::ARCHIVE_EXTENSION
        ));

        let progress = |phase: &'static str, received: u64, total: u64| {
            manager.emit(ManagerEvent::InstallProgress {
                id: id.to_owned(),
                phase,
                received,
                total,
            });
        };
        progress("downloading", 0, version.size);
        let mut last = Instant::now();
        registry.download(version, &file, &self.cancel, &mut |received, total| {
            if last.elapsed() >= PROGRESS_STEP {
                last = Instant::now();
                progress("downloading", received, total);
            }
        })?;
        progress("verifying", version.size, version.size);

        // The archive's own manifest must say what the index said: this is
        // what makes the consent text (taken from the index) the truth.
        let report = manager.inspect(&file)?;
        if report.name != id || report.version != version.version {
            return Err(ManagerError::RegistryMismatch(format!(
                "the download is `{}` {}, not `{id}` {}",
                report.name, report.version, version.version
            )));
        }
        if !same_permissions(
            &report.permissions,
            &kernel_permissions(&version.permissions),
        ) {
            return Err(ManagerError::RegistryMismatch(
                "the plugin asks for different access than the registry listed".to_owned(),
            ));
        }

        progress("installing", version.size, version.size);
        manager.install(&file)?;
        // The person agreed to exactly this set before the download.
        let state = manager.approve(id, &report.permissions)?;
        manager.set_origin(
            id,
            Origin {
                registry: registry.base().as_str().to_owned(),
                id: id.to_owned(),
                version: version.version.clone(),
            },
        )?;
        progress("done", version.size, version.size);
        let state = manager.state_of(id).unwrap_or(state);
        Ok(serde_json::to_value(&state).unwrap_or(Value::Null))
    }
}
