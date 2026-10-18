//! Hot reload: watch the plugins directory and keep the kernel in sync.

use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher};

use crate::error::{LoadFailure, ManagerError};
use crate::events::ManagerEvent;
use crate::manager::{PluginInfo, PluginManager, SyncOutcome};

/// How often the loop checks the stop flag and the debounce timer.
const POLL: Duration = Duration::from_millis(20);

/// What the watcher did in response to file changes.
#[derive(Debug)]
pub enum PluginEvent {
    Loaded(PluginInfo),
    Reloaded(PluginInfo),
    Unloaded(String),
    /// Switched off (or hidden by a project): unloaded, files untouched.
    Disabled(String),
    /// Asks for permissions nobody approved: left unloaded.
    NeedsApproval {
        name: String,
        version: String,
    },
    Failed(LoadFailure),
    /// The operating-system watcher reported an error.
    WatchError(String),
}

/// Debounce settings.
#[derive(Debug, Clone, Copy)]
pub struct WatchOptions {
    /// Wait this long after the last change before reloading, so a plugin
    /// being copied or rebuilt is read once, complete.
    pub debounce: Duration,
    /// Reload at the latest this long after the first change, even if files
    /// keep changing.
    pub max_wait: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(250),
            max_wait: Duration::from_secs(3),
        }
    }
}

/// Stops watching when dropped.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // A panicked watcher thread has nothing left to clean up.
            let _ = thread.join();
        }
    }
}

/// The plugin directory name an event path belongs to, if any.
fn plugin_of(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    match rel.components().next()? {
        Component::Normal(name) => {
            let name = name.to_string_lossy().into_owned();
            (!name.starts_with('.')).then_some(name)
        }
        _ => None,
    }
}

impl PluginManager {
    /// Watch the plugins directory. File changes are debounced, then each
    /// affected plugin is loaded, hot-reloaded or unloaded. A reload that
    /// fails keeps the previous version running. Every outcome is reported
    /// through `on_event`, which runs on the watcher thread.
    pub fn watch(
        self: &Arc<Self>,
        options: WatchOptions,
        on_event: impl Fn(PluginEvent) + Send + 'static,
    ) -> Result<WatchHandle, ManagerError> {
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(tx).map_err(|e| ManagerError::Watch {
            path: self.plugins_dir().to_path_buf(),
            reason: e.to_string(),
        })?;
        watcher
            .watch(self.plugins_dir(), RecursiveMode::Recursive)
            .map_err(|e| ManagerError::Watch {
                path: self.plugins_dir().to_path_buf(),
                reason: e.to_string(),
            })?;

        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let manager = Arc::clone(self);
        let thread = thread::Builder::new()
            .name("manang-plugin-watch".to_owned())
            .spawn(move || {
                // Keep the watcher alive for the life of the thread.
                let _watcher = watcher;
                let mut pending: BTreeSet<String> = BTreeSet::new();
                let mut first = Instant::now();
                let mut last = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    match rx.recv_timeout(POLL) {
                        Ok(Ok(event)) => {
                            // Reads of our own (inotify "access" events) must not
                            // retrigger a reload.
                            if matches!(event.kind, EventKind::Access(_)) {
                                continue;
                            }
                            let before = pending.len();
                            for path in &event.paths {
                                if let Some(name) = plugin_of(manager.plugins_dir(), path) {
                                    pending.insert(name);
                                }
                            }
                            if !pending.is_empty() {
                                if before == 0 {
                                    first = Instant::now();
                                }
                                last = Instant::now();
                            }
                        }
                        Ok(Err(e)) => {
                            manager.emit(ManagerEvent::WatchError {
                                error: e.to_string(),
                            });
                            on_event(PluginEvent::WatchError(e.to_string()));
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    let settled =
                        last.elapsed() >= options.debounce || first.elapsed() >= options.max_wait;
                    if !pending.is_empty() && settled {
                        for name in std::mem::take(&mut pending) {
                            match manager.sync(&name) {
                                SyncOutcome::Loaded(i) => on_event(PluginEvent::Loaded(i)),
                                SyncOutcome::Reloaded(i) => on_event(PluginEvent::Reloaded(i)),
                                SyncOutcome::Unloaded(n) => on_event(PluginEvent::Unloaded(n)),
                                SyncOutcome::Disabled { name, .. } => {
                                    on_event(PluginEvent::Disabled(name));
                                }
                                SyncOutcome::NeedsApproval { name, version } => {
                                    on_event(PluginEvent::NeedsApproval { name, version });
                                }
                                SyncOutcome::Failed(f) => on_event(PluginEvent::Failed(f)),
                                SyncOutcome::Unchanged(_) | SyncOutcome::Absent => {}
                            }
                        }
                    }
                }
            })
            .map_err(|e| ManagerError::Watch {
                path: self.plugins_dir().to_path_buf(),
                reason: e.to_string(),
            })?;
        Ok(WatchHandle {
            stop,
            thread: Some(thread),
        })
    }
}
