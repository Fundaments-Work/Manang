//! The `visual` plugin: sessions of the Visual editor as kernel commands.
//!
//! `manang-visual` is the pure core (adapter traits, span-table encoding,
//! session host); this crate is the OS-side glue that makes it a first-party
//! plugin: it reads document text from the editor plugin, runs every call on
//! a private Rayon pool (AGENTS.md: parsing is CPU-bound, never on Tokio or
//! the UI thread) and answers `visual.blocks` with raw bytes, which the shell
//! hands to Tauri as a `tauri::ipc::Response`. The wire contract is frozen in
//! `docs/adr/0015-visual-editor.md` and mirrored in
//! `src/lib/editor/visual/protocol.ts`.

mod plugin;

pub use plugin::{VisualHostError, VisualPlugin};

use manang_kernel::Manifest;

const MANIFEST: &str = include_str!("../manifest.toml");

/// The plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}
