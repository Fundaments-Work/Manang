//! Manang's first-party settings plugin.
//!
//! * a typed [`registry`] other plugins add to,
//! * layered values (defaults < user `settings.toml` < project
//!   `.manang/settings.toml`) in hand-editable, comment-preserving TOML,
//! * keybindings with conflict detection,
//! * brand-locked themes with contrast validation.
//!
//! See [`plugin`] for the command surface and event topics.

pub mod defaults;
pub mod error;
pub mod keybindings;
pub mod keys;
pub mod layers;
mod plugin;
pub mod registry;
pub mod store;
pub mod sync;
pub mod theme;

pub use error::{KeyError, SettingsError, ValidationError};
pub use layers::{Resolved, Source, Warning};
pub use plugin::{
    DEFAULT_DEBOUNCE, KEYBINDINGS_FILE, PROJECT_DIR, SETTINGS_FILE, SettingsPlugin, THEMES_DIR,
    TOPIC_CHANGED, TOPIC_KEYBINDINGS, TOPIC_PROJECT, TOPIC_REGISTERED, TOPIC_WARNINGS,
    keybindings_manifest, manifest, manifests, plugin, theme_manifest,
};
pub use registry::{Registry, Scope, SettingDef, SettingType};
pub use sync::{PluginSettingsSync, SyncError, SyncOutcome};
