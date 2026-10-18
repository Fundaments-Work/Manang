//! Manang's first-party git plugin.
//!
//! Reads use pure-Rust `gix`; writes and network sync drive the system `git`
//! binary under a sanitised environment (see `docs/adr/0005-git-backend.md`).
//! Every path is confined to the project root with `manang_project`'s
//! [`ProjectRoot`](manang_project::ProjectRoot).

pub mod ctx;
pub mod diff;
mod error;
pub mod history;
pub mod identity;
mod plugin;
pub mod remote;
pub mod restore;
pub mod runner;
pub mod status;
pub mod versions;
pub mod writes;

pub use error::GitError;
pub use plugin::{GitPlugin, TOPIC_CHANGED, manifest, plugin};
