//! Manang's plugin registry client.
//!
//! The registry is a static host serving `index.json` (the catalogue),
//! `index.json.sig` (an ed25519 signature over its exact bytes), `stats.json`
//! and the mirrored plugin archives. This crate fetches and verifies the
//! index against the public keys embedded in the app ([`TrustedKeys`]),
//! validates its structure ([`parse_index`]), caches the last verified copy
//! ([`Cache`]), searches it ([`search`]) and downloads archives with size and
//! SHA-256 checks ([`Registry::download`]). It installs nothing: the plugin
//! manager takes a verified file through its own archive, consent and
//! sandbox checks (ADR 0016).
//!
//! Web-capable (AGENTS.md invariant 5): no OS-only dependency beyond the
//! optional `native` feature, which supplies the HTTP [`Transport`]. The clock
//! is a parameter, never read here.

mod cache;
mod client;
mod error;
mod index;
mod keys;
mod query;
mod transport;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

#[cfg(all(test, feature = "native"))]
mod client_tests;

pub use cache::{Cache, Meta, Stats, Stored};
pub use client::{Freshness, Listing, Registry, sha256_hex};
pub use error::RegistryError;
pub use index::{
    Index, MAX_ARTIFACT_BYTES, MAX_INDEX_BYTES, Permissions, PluginEntry, SCHEMA, VersionEntry,
    app_satisfies, compare_versions, is_newer, is_safe_relative_path, parse_index,
};
pub use keys::TrustedKeys;
pub use query::{Query, Sort, Update, search, tags, updates};
#[cfg(feature = "native")]
pub use transport::HttpTransport;
pub use transport::{Base, DEFAULT_URL, PLACEHOLDER_URL, Response, Transport};
