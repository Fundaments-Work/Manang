use crate::error::KeystoreError;
use crate::store::{KeyId, KeyStore, StoreKind};
use keyring_core::{CredentialStore, Entry, Error};
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;

/// A [`KeyStore`] over any `keyring-core` credential store. Entries are built
/// from the store directly: no process-wide default store is installed.
pub struct KeyringStore {
    kind: StoreKind,
    store: Arc<CredentialStore>,
}

impl std::fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyringStore")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl KeyringStore {
    pub fn new(kind: StoreKind, store: Arc<CredentialStore>) -> Self {
        Self { kind, store }
    }

    fn entry(&self, id: &KeyId) -> Result<Entry, KeystoreError> {
        self.store
            .build(KeyId::SERVICE, &id.entry_user(), None)
            .map_err(|e| self.backend_error(&e))
    }

    /// Uses the error's `Display` only: its `Debug` can carry secret bytes
    /// (`BadEncoding`).
    fn backend_error(&self, error: &Error) -> KeystoreError {
        KeystoreError::Backend {
            store: self.kind.as_str(),
            message: error.to_string(),
        }
    }
}

impl KeyStore for KeyringStore {
    fn kind(&self) -> StoreKind {
        self.kind
    }

    fn set(&self, id: &KeyId, key: &SecretString) -> Result<(), KeystoreError> {
        self.entry(id)?
            .set_password(key.expose_secret())
            .map_err(|e| self.backend_error(&e))
    }

    fn get(&self, id: &KeyId) -> Result<Option<SecretString>, KeystoreError> {
        match self.entry(id)?.get_password() {
            Ok(password) => Ok(Some(SecretString::from(password))),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(self.backend_error(&e)),
        }
    }

    fn delete(&self, id: &KeyId) -> Result<bool, KeystoreError> {
        match self.entry(id)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(Error::NoEntry) => Ok(false),
            Err(e) => Err(self.backend_error(&e)),
        }
    }
}

/// The OS stores for this platform, best first. A store that cannot even be
/// opened (no D-Bus session, no Secret Service) is left out.
pub fn detect_platform_stores() -> Vec<Box<dyn KeyStore>> {
    // `mut` is unused only on platforms without an OS store (none we ship to).
    #[allow(unused_mut)]
    let mut stores: Vec<Box<dyn KeyStore>> = Vec::new();

    #[cfg(target_os = "macos")]
    if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
        stores.push(Box::new(KeyringStore::new(StoreKind::Keychain, store)));
    }

    #[cfg(target_os = "windows")]
    if let Ok(store) = windows_native_keyring_store::Store::new() {
        stores.push(Box::new(KeyringStore::new(StoreKind::Keychain, store)));
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(store) = zbus_secret_service_keyring_store::Store::new() {
            stores.push(Box::new(KeyringStore::new(StoreKind::Keychain, store)));
        }
        if let Ok(store) = linux_keyutils_keyring_store::Store::new() {
            stores.push(Box::new(KeyringStore::new(StoreKind::Kernel, store)));
        }
    }

    stores
}
