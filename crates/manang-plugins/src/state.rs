//! What the Plugins view shows: one [`PluginState`] per plugin, built, on
//! demand, from the manifest on disk, the user's decisions, the running
//! kernel and the failures the manager kept.

use manang_kernel::Manifest;
use manang_kernel::Tier;
use manang_kernel::manifest::Permissions;
use serde::Serialize;

use crate::error::ManagerError;
use crate::manager::{DisabledBy, PluginManager, read_manifest};
use crate::perms;

/// Where a plugin stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Loaded,
    Disabled,
    Failed,
    /// Asks for permissions nobody has approved yet.
    NeedsApproval,
}

/// A command a plugin provides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandRef {
    pub id: String,
    pub title: String,
}

/// One plugin, built in or installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginState {
    pub name: String,
    /// The version on disk (the pending one while an update waits).
    pub version: String,
    /// The version that is running, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_version: Option<String>,
    pub description: String,
    pub tier: Tier,
    /// Required by the app: cannot be disabled or removed.
    pub core: bool,
    /// What the plugin asks for.
    pub permissions: Permissions,
    /// What the user approved (built-ins hold what they declare).
    pub granted: Permissions,
    /// Asked for but not yet approved: what an update adds.
    pub new_permissions: Permissions,
    pub enabled: bool,
    /// `user` or `project` when `enabled` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_by: Option<&'static str>,
    pub status: PluginStatus,
    /// Why it failed, or why an update did not load.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The publisher the plugin declares. Unverified while `signed` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Whether a valid signature vouches for it. Always false for now.
    pub signed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<u64>,
    pub commands: Vec<CommandRef>,
    /// Ids of the settings it declares.
    pub settings: Vec<String>,
    /// Where its Settings section belongs.
    pub contexts: Vec<String>,
    /// A newer version is waiting for approval beside the running one.
    pub update_pending: bool,
}

/// The answer to `plugins.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginList {
    pub plugins: Vec<PluginState>,
    /// Startup problems worth showing (a damaged registry).
    pub warnings: Vec<String>,
}

/// What `plugins.permissions` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionReport {
    pub requested: Permissions,
    pub granted: Permissions,
    pub new_permissions: Permissions,
}

fn commands_of(m: &Manifest) -> Vec<CommandRef> {
    m.contributes
        .commands
        .iter()
        .map(|c| CommandRef {
            id: c.id.clone(),
            title: c.title.clone(),
        })
        .collect()
}

fn builtin(m: &Manifest, core: bool, off: bool, disabled_by: Option<DisabledBy>) -> PluginState {
    PluginState {
        name: m.plugin.name.clone(),
        version: m.plugin.version.to_string(),
        running_version: (!off).then(|| m.plugin.version.to_string()),
        description: m.plugin.description.clone(),
        tier: m.plugin.tier,
        core,
        permissions: m.permissions.clone(),
        granted: m.permissions.clone(),
        new_permissions: Permissions::default(),
        enabled: !off,
        disabled_by: disabled_by.map(by_label),
        status: if off {
            PluginStatus::Disabled
        } else {
            PluginStatus::Loaded
        },
        error: None,
        dir: None,
        source: None,
        publisher: m.plugin.publisher.clone(),
        signed: false,
        installed_at: None,
        commands: commands_of(m),
        settings: m
            .contributes
            .settings
            .iter()
            .map(|s| s.id.clone())
            .collect(),
        contexts: m.settings_contexts(),
        update_pending: false,
    }
}

fn by_label(by: DisabledBy) -> &'static str {
    match by {
        DisabledBy::User => "user",
        DisabledBy::Project => "project",
    }
}

impl PluginManager {
    /// Every plugin, built in and installed, sorted by name.
    pub fn list(&self) -> Vec<PluginState> {
        let mut out = Vec::new();
        let wasm: std::collections::BTreeSet<String> = self.loaded.lock().keys().cloned().collect();
        for name in self.kernel.plugin_names() {
            if wasm.contains(&name) {
                continue;
            }
            if let Some(m) = self.kernel.manifest(&name) {
                out.push(builtin(&m, self.core.contains(&name), false, None));
            }
        }
        let off: Vec<Manifest> = self
            .natives_off
            .lock()
            .values()
            .map(|(m, _)| m.clone())
            .collect();
        for m in off {
            let by = self.disabled_by(&m.plugin.name);
            out.push(builtin(&m, false, true, by));
        }
        if let Ok(dirs) = self.candidates() {
            for dir in dirs {
                out.push(self.state_of_dir(&dir));
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// [`PluginManager::list`] plus startup warnings.
    pub fn list_with_warnings(&self) -> PluginList {
        PluginList {
            plugins: self.list(),
            warnings: self.warnings(),
        }
    }

    /// One plugin's state.
    pub fn state_of(&self, name: &str) -> Result<PluginState, ManagerError> {
        self.list()
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| ManagerError::NotInstalled(name.to_owned()))
    }

    /// What `name` asks for, has been granted, and is waiting on.
    pub fn permissions(&self, name: &str) -> Result<PermissionReport, ManagerError> {
        let s = self.state_of(name)?;
        Ok(PermissionReport {
            requested: s.permissions,
            granted: s.granted,
            new_permissions: s.new_permissions,
        })
    }

    fn state_of_dir(&self, dir: &std::path::Path) -> PluginState {
        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let record = self.registry.lock().get(&dir_name).cloned();
        let failure = self.failures.lock().get(&dir_name).cloned();
        let held = self.held.lock().contains_key(&dir_name);
        let running = self
            .loaded
            .lock()
            .get(&dir_name)
            .map(|l| l.info.version.clone());
        let pending_dir = self.root.join(format!(".pending-{dir_name}"));
        let pending =
            record.as_ref().is_some_and(|r| r.pending_version.is_some()) && pending_dir.is_dir();
        let disabled_by = self.disabled_by(&dir_name);

        let read = read_manifest(if pending { &pending_dir } else { dir });
        let (manifest, parse_error) = match read {
            Ok((m, _)) => (Some(m), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let requested = manifest
            .as_ref()
            .map(|m| m.permissions.clone())
            .unwrap_or_default();
        // Not running and never approved: either recorded without approval
        // (just installed) or not recorded yet and asking for access.
        let never_approved = match &record {
            Some(r) => r.approved_version.is_none(),
            None => !perms::is_empty(&requested),
        };
        let needs_approval = held || pending || (running.is_none() && never_approved);
        let reviewed = record
            .as_ref()
            .map(|r| r.reviewed.clone())
            .unwrap_or_default();
        let new_permissions = if needs_approval {
            perms::difference(&requested, &reviewed)
        } else {
            Permissions::default()
        };
        let error = failure
            .as_ref()
            .map(|f| f.error.clone())
            .or(parse_error.clone());
        let status = if disabled_by.is_some() {
            PluginStatus::Disabled
        } else if needs_approval {
            PluginStatus::NeedsApproval
        } else if running.is_some() {
            PluginStatus::Loaded
        } else if error.is_some() {
            PluginStatus::Failed
        } else {
            PluginStatus::Disabled
        };
        let m = manifest.as_ref();
        PluginState {
            name: m.map_or(dir_name.clone(), |m| m.plugin.name.clone()),
            version: m.map_or_else(String::new, |m| m.plugin.version.to_string()),
            running_version: running,
            description: m.map_or_else(String::new, |m| m.plugin.description.clone()),
            tier: Tier::Wasm,
            core: self.core.contains(&dir_name),
            permissions: requested,
            granted: record
                .as_ref()
                .map(|r| r.granted.clone())
                .unwrap_or_default(),
            new_permissions,
            enabled: disabled_by.is_none(),
            disabled_by: disabled_by.map(by_label),
            status,
            error,
            dir: Some(dir.display().to_string()),
            source: record.as_ref().and_then(|r| r.source.clone()),
            publisher: record
                .as_ref()
                .and_then(|r| r.publisher.clone())
                .or_else(|| m.and_then(|m| m.plugin.publisher.clone())),
            signed: false,
            installed_at: record.as_ref().map(|r| r.installed_at).filter(|t| *t > 0),
            commands: m.map(commands_of).unwrap_or_default(),
            settings: m
                .map(|m| {
                    m.contributes
                        .settings
                        .iter()
                        .map(|s| s.id.clone())
                        .collect()
                })
                .unwrap_or_default(),
            contexts: m.map(Manifest::settings_contexts).unwrap_or_default(),
            update_pending: pending || (held && record.is_some()),
        }
    }
}
