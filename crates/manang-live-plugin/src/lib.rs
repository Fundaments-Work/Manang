//! The `live` plugin: real-time co-editing of one open file.
//!
//! `manang-live` (the CRDT, the encrypted envelope, the session state machine)
//! is pure and web-capable; `manang-relay` carries the encrypted frames. This
//! crate is the OS-side glue that makes them a first-party plugin: it runs a
//! session on its own thread, keeps the editor plugin's shared copy of the
//! file in step with it, and talks to the web view through commands and
//! `live.*` events. See `driver` for the protocol with the web view and
//! `docs/adr/0013-live-editing.md` for the design.
//!
//! The file on disk stays plain UTF-8: the CRDT lives only in memory and in
//! the session.

mod driver;
mod error;
mod invite;
mod plugin;

pub use driver::{Change, PeerInfo, PushOutcome, Status};
pub use error::LiveHostError;
pub use invite::{InviteLink, normalize_relay};
pub use plugin::LivePlugin;

use manang_kernel::Manifest;

const MANIFEST: &str = include_str!("../manifest.toml");

/// The plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}
