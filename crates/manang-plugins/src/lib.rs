//! Discovery, loading and hot reload of Manang WASM plugins.
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

mod error;
mod manager;
mod watch;

pub use error::{LoadError, LoadFailure, ManagerError};
pub use manager::{
    DEFAULT_ENTRY, MANIFEST_FILE, MAX_WASM_BYTES, ManagerConfig, PluginInfo, PluginManager,
    ScanReport, SyncOutcome, cache_dir_in, plugins_dir_in,
};
pub use watch::{PluginEvent, WatchHandle, WatchOptions};
