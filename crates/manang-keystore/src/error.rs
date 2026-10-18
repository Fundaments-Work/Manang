use thiserror::Error;

/// Keystore failures. Messages never contain a key.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeystoreError {
    /// The provider or account name is not acceptable.
    #[error("invalid {field}: {reason}")]
    InvalidId {
        field: &'static str,
        reason: &'static str,
    },
    /// The key is empty or too long.
    #[error("invalid key: {0}")]
    InvalidKey(&'static str),
    /// The platform store failed; the text is the platform's own message.
    #[error("{store} store unavailable: {message}")]
    Backend {
        store: &'static str,
        message: String,
    },
    /// No backend could hold the key.
    #[error("no key store could save the key")]
    NoStore,
}
