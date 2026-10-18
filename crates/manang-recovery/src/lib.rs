//! Crash recovery ("hot exit") for unsaved edits.
//!
//! While a document has unsaved changes the shell asks this plugin to keep a
//! snapshot of it. A snapshot is a small, human-readable JSON file under
//! `<root>/<project-id>/<file-id>.json`, written atomically (temp file, sync,
//! rename), so a crash or power loss never leaves a torn one. A successful
//! save or an explicit discard removes it. Recovery never touches the user's
//! file: after a crash the snapshot's text is offered back and opens as an
//! unsaved edit.
//!
//! Snapshots over [`MAX_SNAPSHOT_BYTES`] are not stored (the caller is told, so
//! it can say so). A snapshot that cannot be parsed is renamed to
//! `<name>.json.corrupt` and logged, never fatal, never deleted. A snapshot
//! written by a newer Manang is left untouched and not offered.
//!
//! Commands: `recovery.write`, `recovery.list`, `recovery.read`,
//! `recovery.discard`.

mod plugin;
mod store;

pub use plugin::{RecoveryPlugin, manifest, plugin};
pub use store::{
    DiskState, Entry, Kind, Listing, MAX_SNAPSHOT_BYTES, RecoveryError, RecoveryStore, Snapshot,
    WriteOutcome, WriteRequest,
};
