use crate::error::KeystoreError;
use crate::store::{KeyId, KeyStatus, KeyStore, StoreKind};
use secrecy::SecretString;

/// Tries stores in order. A key is saved in the first store that accepts it
/// (a locked keychain falls through to the kernel keyring, then to memory) and
/// removed from the others so no stale copy lingers.
pub struct ChainKeyStore {
    backends: Vec<Box<dyn KeyStore>>,
}

impl std::fmt::Debug for ChainKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kinds: Vec<&str> = self.backends.iter().map(|b| b.kind().as_str()).collect();
        f.debug_struct("ChainKeyStore")
            .field("backends", &kinds)
            .finish()
    }
}

impl ChainKeyStore {
    pub fn new(backends: Vec<Box<dyn KeyStore>>) -> Self {
        Self { backends }
    }

    /// The store kinds in selection order.
    pub fn order(&self) -> Vec<StoreKind> {
        self.backends.iter().map(|b| b.kind()).collect()
    }

    /// Saves the key and reports where it went.
    pub fn set(&self, id: &KeyId, key: &SecretString) -> Result<StoreKind, KeystoreError> {
        for (index, backend) in self.backends.iter().enumerate() {
            if backend.set(id, key).is_ok() {
                // Drop copies held by every other store; failures here are
                // not worth failing the save for.
                for (other, store) in self.backends.iter().enumerate() {
                    if other != index {
                        let _ = store.delete(id);
                    }
                }
                return Ok(backend.kind());
            }
        }
        Err(KeystoreError::NoStore)
    }

    /// The first stored key, for the provider layer.
    pub fn get(&self, id: &KeyId) -> Result<Option<SecretString>, KeystoreError> {
        for backend in &self.backends {
            if let Ok(Some(key)) = backend.get(id) {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }

    /// Removes the key everywhere; true if any store had one. If a store
    /// failed, the key may remain there, so the first failure is returned.
    pub fn delete(&self, id: &KeyId) -> Result<bool, KeystoreError> {
        let mut removed = false;
        let mut failure = None;
        for backend in &self.backends {
            match backend.delete(id) {
                Ok(existed) => removed |= existed,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(removed),
        }
    }

    /// Whether a key exists, and which store holds it (or would).
    pub fn status(&self, id: &KeyId) -> Result<KeyStatus, KeystoreError> {
        for backend in &self.backends {
            if let Ok(Some(_)) = backend.get(id) {
                return Ok(KeyStatus {
                    present: true,
                    store: backend.kind(),
                });
            }
        }
        let store = self
            .backends
            .first()
            .map_or(StoreKind::Session, |b| b.kind());
        Ok(KeyStatus {
            present: false,
            store,
        })
    }
}
