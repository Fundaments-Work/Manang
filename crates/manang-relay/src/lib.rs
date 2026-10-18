//! A ciphertext-only WebSocket relay for Manang live editing.
//!
//! The relay is deliberately dumb. It knows a *room id* (a one-way hash of the
//! room secret, so it cannot decrypt anything), keeps a set of connections per
//! room, and forwards each binary frame it receives to every other connection
//! in the same room. It never parses, stores or logs payloads, keeps nothing on
//! disk, and forgets a room when its last peer leaves. Late joiners catch up
//! from the other peers (see `manang_live::session`), not from the relay.
//!
//! It validates only what it can see without the key: frame size, the
//! envelope's version and kind bytes, and per-connection rates. Everything
//! else is the peers' business, and every peer treats the relay as hostile.
//!
//! Wire contract: connect a WebSocket to `/room/<32 lowercase hex chars>` and
//! exchange binary messages. `GET /health` answers `ok` for load balancers.

mod client;
mod config;
mod server;

pub use client::RelayClient;
pub use config::{FrameTap, RelayConfig};
pub use server::{RunningRelay, spawn};
