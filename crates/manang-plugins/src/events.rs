//! What the manager tells the shell happened. Every event is plain data with
//! a topic and a JSON payload, so the shell can put it on the event bus
//! without knowing the manager's types.

use std::sync::Arc;

use serde_json::{Value, json};

/// A change in the set of plugins or their state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerEvent {
    /// A scan of the plugins directory finished.
    Scanned {
        loaded: usize,
        failed: usize,
    },
    Loaded {
        name: String,
    },
    Reloaded {
        name: String,
    },
    Unloaded {
        name: String,
    },
    /// A plugin could not be loaded. `name` is the plugin's, or its
    /// directory's when the manifest was unreadable.
    Failed {
        name: String,
        error: String,
    },
    /// A plugin waits for the user to approve its permissions.
    NeedsApproval {
        name: String,
        version: String,
    },
    /// Something about the plugin changed (enabled, approved, installed,
    /// removed): refresh it.
    StateChanged {
        name: String,
    },
    /// The operating-system file watcher reported an error.
    WatchError {
        error: String,
    },
}

impl ManagerEvent {
    /// The event-bus topic: `plugins.<what>`.
    pub fn topic(&self) -> &'static str {
        match self {
            Self::Scanned { .. } => "plugins.scanned",
            Self::Loaded { .. } => "plugins.loaded",
            Self::Reloaded { .. } => "plugins.reloaded",
            Self::Unloaded { .. } => "plugins.unloaded",
            Self::Failed { .. } => "plugins.failed",
            Self::NeedsApproval { .. } => "plugins.needs-approval",
            Self::StateChanged { .. } => "plugins.state-changed",
            Self::WatchError { .. } => "plugins.watch-error",
        }
    }

    /// The plugin the event is about, if any.
    pub fn plugin(&self) -> Option<&str> {
        match self {
            Self::Loaded { name }
            | Self::Reloaded { name }
            | Self::Unloaded { name }
            | Self::Failed { name, .. }
            | Self::NeedsApproval { name, .. }
            | Self::StateChanged { name } => Some(name),
            Self::Scanned { .. } | Self::WatchError { .. } => None,
        }
    }

    pub fn payload(&self) -> Value {
        match self {
            Self::Scanned { loaded, failed } => json!({ "loaded": loaded, "failed": failed }),
            Self::Loaded { name } | Self::Reloaded { name } | Self::Unloaded { name } => {
                json!({ "name": name })
            }
            Self::Failed { name, error } => json!({ "name": name, "error": error }),
            Self::NeedsApproval { name, version } => {
                json!({ "name": name, "version": version })
            }
            Self::StateChanged { name } => json!({ "name": name }),
            Self::WatchError { error } => json!({ "error": error }),
        }
    }
}

/// Receives every [`ManagerEvent`], on whichever thread caused it. Must not
/// block and must not call back into the manager.
pub type Listener = Arc<dyn Fn(&ManagerEvent) + Send + Sync>;
