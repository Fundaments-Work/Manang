//! Delivers plugin settings to the plugins that declare them (ADR 0007).
//!
//! A plugin declares settings in its manifest (`[[contributes.settings]]`)
//! and provides `<plugin>.configure {values: {id: value}}`. This component:
//!
//! 1. registers a plugin's declared settings with `settings.register`
//!    (owner = plugin name) and pushes their effective values to its
//!    `configure` command ([`PluginSettingsSync::sync_plugin`]);
//! 2. runs a listener thread ([`PluginSettingsSync::start`]) that re-pushes
//!    the owning plugin's values on `settings.changed` and every plugin's
//!    values on `settings.project-changed`.
//!
//! The listener holds only a [`Weak`] kernel reference and a bounded bus
//! subscription, so it never keeps the kernel alive and never blocks an
//! emitter. It exits when [`PluginSettingsSync::stop`] is called (or the
//! sync is dropped), when the kernel is dropped, or when the bus is dropped.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use manang_kernel::{Event, EventBus, Kernel, KernelError, Manifest, Payload};
use parking_lot::Mutex;
use serde_json::{Map, Value, json};

use crate::plugin::{TOPIC_CHANGED, TOPIC_PROJECT};
use crate::registry::SettingDef;

/// Queue depth of the listener's subscription. A burst larger than this
/// (a project switch touching hundreds of settings) loses individual
/// `settings.changed` events, but `settings.project-changed` follows such a
/// burst and re-pushes everything.
pub const SYNC_QUEUE: usize = 512;

/// How often the listener wakes to check its stop conditions.
const POLL: Duration = Duration::from_millis(200);

/// Why a sync step failed.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("the kernel has been dropped")]
    KernelGone,
    #[error("no plugin named `{0}` is registered")]
    UnknownPlugin(String),
    #[error(transparent)]
    Kernel(#[from] KernelError),
    #[error("`settings.get` returned an unexpected payload")]
    BadValues,
    #[error("could not start the settings sync thread: {0}")]
    Spawn(#[from] std::io::Error),
}

/// What [`PluginSettingsSync::sync_plugin`] did for one plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncOutcome {
    /// How many settings the plugin declares.
    pub declared: usize,
    /// Whether values were pushed to `<plugin>.configure`.
    pub pushed: bool,
}

struct Shared {
    kernel: Weak<Kernel>,
    stop: AtomicBool,
}

impl Shared {
    fn kernel(&self) -> Result<Arc<Kernel>, SyncError> {
        self.kernel.upgrade().ok_or(SyncError::KernelGone)
    }

    fn manifest(&self, kernel: &Kernel, name: &str) -> Result<Manifest, SyncError> {
        kernel
            .manifest(name)
            .ok_or_else(|| SyncError::UnknownPlugin(name.to_owned()))
    }

    fn sync_plugin(&self, name: &str) -> Result<SyncOutcome, SyncError> {
        let kernel = self.kernel()?;
        let manifest = self.manifest(&kernel, name)?;
        let declared = manifest.contributes.settings.len();
        if declared == 0 {
            return Ok(SyncOutcome::default());
        }
        let contexts = manifest.settings_contexts();
        let defs: Vec<SettingDef> = manifest
            .contributes
            .settings
            .iter()
            .map(|spec| SettingDef {
                contexts: contexts.clone(),
                ..SettingDef::from(spec)
            })
            .collect();
        let defs = serde_json::to_value(defs).map_err(|_| SyncError::BadValues)?;
        kernel.invoke(
            "settings.register",
            Payload::Json(json!({"settings": defs, "owner": name})),
        )?;
        let pushed = push(&kernel, &manifest)?;
        Ok(SyncOutcome { declared, pushed })
    }

    fn push_plugin(&self, name: &str) -> Result<bool, SyncError> {
        let kernel = self.kernel()?;
        let manifest = self.manifest(&kernel, name)?;
        push(&kernel, &manifest)
    }

    fn push_all(&self) -> Vec<(String, Result<bool, SyncError>)> {
        let Ok(kernel) = self.kernel() else {
            return Vec::new();
        };
        kernel
            .plugin_names()
            .into_iter()
            .filter_map(|name| kernel.manifest(&name).map(|m| (name, m)))
            .filter(|(_, m)| !m.contributes.settings.is_empty())
            .map(|(name, m)| {
                let result = push(&kernel, &m);
                (name, result)
            })
            .collect()
    }
}

/// Push the effective values of `manifest`'s settings to its `configure`
/// command. `Ok(false)` when it declares no settings or has no such command.
fn push(kernel: &Kernel, manifest: &Manifest) -> Result<bool, SyncError> {
    let configure = manifest.configure_command();
    let has_configure = manifest
        .contributes
        .commands
        .iter()
        .any(|c| c.id == configure);
    if manifest.contributes.settings.is_empty() || !has_configure {
        return Ok(false);
    }
    let Payload::Json(all) = kernel.invoke("settings.get", Payload::Json(json!({})))? else {
        return Err(SyncError::BadValues);
    };
    let Some(effective) = all.get("values").and_then(Value::as_object) else {
        return Err(SyncError::BadValues);
    };
    let values: Map<String, Value> = manifest
        .contributes
        .settings
        .iter()
        .filter_map(|s| {
            let value = effective.get(&s.id)?.get("value")?;
            Some((s.id.clone(), value.clone()))
        })
        .collect();
    kernel.invoke(&configure, Payload::Json(json!({"values": values})))?;
    Ok(true)
}

/// The plugin owning a setting id: everything before the first dot.
fn owner_of(id: &str) -> Option<&str> {
    id.split_once('.').map(|(owner, _)| owner)
}

/// What a batch of events asks the listener to do.
#[derive(Debug, Default, PartialEq, Eq)]
struct Work {
    all: bool,
    owners: BTreeSet<String>,
}

impl Work {
    fn add(&mut self, event: &Event) {
        if event.topic == TOPIC_PROJECT {
            self.all = true;
        } else if event.topic == TOPIC_CHANGED
            && let Payload::Json(payload) = &event.payload
            && let Some(owner) = payload.get("id").and_then(Value::as_str).and_then(owner_of)
        {
            self.owners.insert(owner.to_owned());
        }
    }
}

/// Registers plugin-declared settings and keeps plugins configured.
pub struct PluginSettingsSync {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl PluginSettingsSync {
    /// A sync for `kernel`. Holds only a weak reference to it.
    pub fn new(kernel: &Arc<Kernel>) -> Self {
        Self {
            shared: Arc::new(Shared {
                kernel: Arc::downgrade(kernel),
                stop: AtomicBool::new(false),
            }),
            thread: Mutex::new(None),
        }
    }

    /// Register `name`'s declared settings and push their effective values
    /// to `<name>.configure` when the plugin provides it. Idempotent; call it
    /// again after a plugin is (re)loaded.
    pub fn sync_plugin(&self, name: &str) -> Result<SyncOutcome, SyncError> {
        self.shared.sync_plugin(name)
    }

    /// [`Self::sync_plugin`] for every registered plugin, in registration
    /// order. One plugin failing does not stop the others.
    pub fn sync_all(&self) -> Vec<(String, Result<SyncOutcome, SyncError>)> {
        let Ok(kernel) = self.shared.kernel() else {
            return Vec::new();
        };
        kernel
            .plugin_names()
            .into_iter()
            .map(|name| {
                let result = self.shared.sync_plugin(&name);
                (name, result)
            })
            .collect()
    }

    /// Push `name`'s current values without registering. `Ok(false)` when
    /// the plugin declares no settings or has no `configure` command.
    pub fn push_plugin(&self, name: &str) -> Result<bool, SyncError> {
        self.shared.push_plugin(name)
    }

    /// Start the listener thread on `events`. Calling it again while the
    /// listener runs does nothing.
    pub fn start(&self, events: &EventBus) -> Result<(), SyncError> {
        let mut thread = self.thread.lock();
        if thread.as_ref().is_some_and(|t| !t.is_finished()) {
            return Ok(());
        }
        self.shared.stop.store(false, Ordering::SeqCst);
        let subscription = events.subscribe_with_capacity("settings.*", SYNC_QUEUE);
        let shared = Arc::clone(&self.shared);
        let handle = std::thread::Builder::new()
            .name("manang-settings-sync".to_owned())
            .spawn(move || {
                loop {
                    if shared.stop.load(Ordering::SeqCst)
                        || shared.kernel.strong_count() == 0
                        || subscription.is_closed()
                    {
                        return;
                    }
                    let Some(first) = subscription.recv_timeout(POLL) else {
                        continue;
                    };
                    // Coalesce a burst: one push per plugin.
                    let mut work = Work::default();
                    work.add(&first);
                    for event in subscription.drain() {
                        work.add(&event);
                    }
                    if work.all {
                        // Failures are per plugin and already isolated; a
                        // plugin that rejects its values keeps its old ones.
                        let _ = shared.push_all();
                    } else {
                        for owner in &work.owners {
                            // Core settings (`editor.*`, ...) belong to no
                            // plugin with a manifest; that is not an error.
                            let _ = shared.push_plugin(owner);
                        }
                    }
                }
            })?;
        *thread = Some(handle);
        Ok(())
    }

    /// Stop the listener and wait for it to exit (at most one poll period).
    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.lock().take() {
            let _ = handle.join();
        }
    }

    /// Whether the listener thread is running.
    pub fn is_running(&self) -> bool {
        self.thread
            .lock()
            .as_ref()
            .is_some_and(|t| !t.is_finished())
    }
}

impl Drop for PluginSettingsSync {
    /// Signal the listener to exit without waiting for it.
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(topic: &str, payload: Value) -> Event {
        Event {
            topic: topic.to_owned(),
            payload: Payload::Json(payload),
        }
    }

    #[test]
    fn work_coalesces_owners_and_project_switches() {
        let mut work = Work::default();
        work.add(&ev(TOPIC_CHANGED, json!({"id": "latex.engine"})));
        work.add(&ev(TOPIC_CHANGED, json!({"id": "latex.latexmk"})));
        work.add(&ev(TOPIC_CHANGED, json!({"id": "search.exclude"})));
        work.add(&ev("settings.registered", json!({"owner": "x"})));
        work.add(&ev(TOPIC_CHANGED, json!({"nope": 1})));
        assert!(!work.all);
        assert_eq!(
            work.owners.iter().map(String::as_str).collect::<Vec<_>>(),
            ["latex", "search"]
        );
        work.add(&ev(TOPIC_PROJECT, json!({"path": null})));
        assert!(work.all);
    }

    #[test]
    fn owner_is_the_first_segment() {
        assert_eq!(owner_of("latex.timeout-seconds"), Some("latex"));
        assert_eq!(owner_of("a.b.c"), Some("a"));
        assert_eq!(owner_of("plain"), None);
    }
}
