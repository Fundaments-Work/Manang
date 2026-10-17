//! The editor plugin: owns open documents and exposes them as commands.
//!
//! Documents are `ropey::Rope`s addressed by character index, so edits stay
//! cheap on multi-megabyte files. The same [`Editor`] doubles as the
//! [`BufferService`] other plugins reach (through capability-gated host
//! interfaces) to read and replace the active document.

mod editor;
mod error;
mod plugin;

pub use editor::{DocId, DocInfo, Editor};
pub use error::EditorError;
pub use plugin::{EditorPlugin, manifest};
