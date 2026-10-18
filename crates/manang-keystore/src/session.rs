use crate::error::KeystoreError;
use crate::store::{KeyId, KeyStore, StoreKind};
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;

/// Keeps keys in memory only. `SecretString` zeroises each value when it is
/// dropped, so deleting a key, replacing it, or dropping the store wipes it.
/// Its status is "session": not saved.
#[derive(Default)]
pub struct SessionKeyStore {
    keys: Mutex<HashMap<KeyId, SecretString>>,
}

impl SessionKeyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for SessionKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionKeyStore")
            .field("keys", &self.keys.lock().len())
            .finish()
    }
}

impl KeyStore for SessionKeyStore {
    fn kind(&self) -> StoreKind {
        StoreKind::Session
    }

    fn set(&self, id: &KeyId, key: &SecretString) -> Result<(), KeystoreError> {
        // A fresh copy: the caller's value is zeroised by its own owner.
        let copy = SecretString::from(key.expose_secret().to_owned());
        self.keys.lock().insert(id.clone(), copy);
        Ok(())
    }

    fn get(&self, id: &KeyId) -> Result<Option<SecretString>, KeystoreError> {
        Ok(self
            .keys
            .lock()
            .get(id)
            .map(|k| SecretString::from(k.expose_secret().to_owned())))
    }

    fn delete(&self, id: &KeyId) -> Result<bool, KeystoreError> {
        Ok(self.keys.lock().remove(id).is_some())
    }
}
