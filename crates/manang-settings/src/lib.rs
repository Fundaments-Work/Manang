//! Manang's first-party settings plugin.
//!
//! * a typed [`registry`] other plugins add to,
//! * layered values (defaults < user `settings.toml` < project
//!   `.manang/settings.toml`) in hand-editable, comment-preserving TOML,
//! * keybindings with conflict detection,
//! * brand-locked themes with contrast validation.
//!
//! See [`plugin`] for the command surface and event topics.

pub mod core;
pub mod error;
pub mod keybindings;
pub mod keys;
pub mod layers;
pub mod registry;
pub mod store;
pub mod theme;

pub use error::{KeyError, SettingsError, ValidationError};
pub use layers::{Resolved, Source, Warning};
pub use registry::{Registry, Scope, SettingDef, SettingType};
