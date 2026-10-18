//! Manang API key storage.
//!
//! Keys live in the OS keychain (macOS Keychain, Windows Credential Manager,
//! the Secret Service on Linux), else the Linux kernel keyring, else in memory
//! for the session. There is NO home-made encrypted file: a key is either
//! held by the OS or it is not saved, and the UI says which.
//!
//! The key never appears in `Debug` output, logs, errors or command results.
//! The only way to read it back is [`KeyStore::get`], for the provider layer
//! inside the process; the `agent.key.*` commands never return it.

mod chain;
mod commands;
mod error;
mod keyring_store;
mod session;
mod store;

pub use chain::ChainKeyStore;
pub use commands::{KeyCommands, KeystorePlugin, manifest, plugin};
pub use error::KeystoreError;
pub use keyring_store::{KeyringStore, detect_platform_stores};
pub use secrecy::{ExposeSecret, SecretString};
pub use session::SessionKeyStore;
pub use store::{KeyId, KeyStatus, KeyStore, StoreKind};

/// The best available chain for this machine: keychain, kernel keyring (Linux),
/// then the in-memory session store.
pub fn detect() -> ChainKeyStore {
    let mut backends = detect_platform_stores();
    backends.push(Box::new(SessionKeyStore::new()));
    ChainKeyStore::new(backends)
}
