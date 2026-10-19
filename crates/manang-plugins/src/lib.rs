//! Discovery, loading, hot reload and lifecycle of Manang WASM plugins.
//!
//! Plugins live in `<config>/manang/plugins/<name>/` as a `manifest.toml` plus
//! the compiled `plugin.wasm` (or the manifest's `plugin.entry`). The
//! directory name must equal the plugin name.
//!
//! [`PluginManager::scan`] loads every directory (compiling in parallel on
//! Rayon) and reports per-plugin failures without aborting the rest.
//! [`PluginManager::watch`] keeps the kernel in sync with the directory:
//! changes are debounced, a reload swaps the plugin atomically
//! ([`manang_kernel::Kernel::replace`]) so calls already running finish on the
//! old instance, and a broken new version never displaces a working one.
//!
//! The lifecycle on top of that:
//!
//! * [`PluginManager::install`] takes a folder or a `.manangplugin` archive (a
//!   zip, see [`archive`]), validates it without running any of it, and parks
//!   it as *needs approval*. [`PluginManager::approve`] records the permissions
//!   the user granted and loads it. An update asking for more waits beside the
//!   running version.
//! * [`PluginManager::enable`] / [`PluginManager::disable`] switch plugins
//!   (including most built-ins) on and off; a project may only switch off
//!   ([`PluginManager::set_project`]).
//! * [`PluginManager::uninstall`] unloads and moves the files to a trash
//!   folder; nothing is deleted.
//! * Decisions persist in `<config>/manang/plugins.toml` ([`registry`]).
//! * [`PolicyGate`] is the seam for an organisation's allow/block rules.
//! * [`manifest`] and [`plugin`] expose all of it as the `plugins.*` commands.

pub mod archive;
mod commands;
mod error;
mod events;
mod install;
mod manager;
pub mod marketplace;
mod perms;
mod policy;
pub mod registry;
mod state;
mod watch;

pub use commands::{PluginsPlugin, manifest, plugin, plugin_with};
pub use error::{LoadError, LoadFailure, ManagerError};
pub use events::{Listener, ManagerEvent};
pub use install::{InspectReport, UninstallReport};
pub use manager::{
    CORE_PLUGINS, DEFAULT_ENTRY, DisabledBy, MANIFEST_FILE, MAX_WASM_BYTES, ManagerConfig,
    PluginInfo, PluginManager, ScanReport, SyncOutcome, cache_dir_in, plugins_dir_in,
    registry_path_in, trash_dir_in,
};
pub use marketplace::{Marketplace, MarketplaceConfig};
pub use perms::{difference as permission_difference, is_subset as permissions_within};
pub use policy::{Permissive, PolicyAction, PolicyDenied, PolicyGate, PolicyRequest};
pub use state::{CommandRef, PermissionReport, PluginList, PluginState, PluginStatus};
pub use watch::{PluginEvent, WatchHandle, WatchOptions};
