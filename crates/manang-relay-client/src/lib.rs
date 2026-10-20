//! The WebSocket client transport for Manang live editing.
//!
//! A relay is a ciphertext-only fan-out: it knows a *room id* (a one-way hash
//! of the room secret), and forwards each binary frame it receives to every
//! other connection in the same room. This crate is the app's side of that:
//! [`RelayClient`] implements `manang_live::transport::SyncTransport` over one
//! WebSocket. The relay server is not part of this repository.
//!
//! Wire contract (public, see ADR 0013 and 0014): connect a WebSocket to
//! `/room/<32 lowercase hex chars>` and exchange binary messages.

mod client;

pub use client::RelayClient;
