use crate::error::KeystoreError;
use secrecy::SecretString;

/// Where a key is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    /// The OS keychain: survives restarts and reboots.
    Keychain,
    /// The Linux kernel keyring: survives app restarts, not a reboot.
    Kernel,
    /// This process's memory: gone when the app closes. NOT saved.
    Session,
}

impl StoreKind {
    /// The value reported by `agent.key.*`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::Kernel => "kernel",
            Self::Session => "session",
        }
    }

    /// Whether the key outlives the app process.
    pub fn is_saved(self) -> bool {
        !matches!(self, Self::Session)
    }
}

/// Identifies one key: a provider (a preset id such as `deepseek`) and an
/// account label (`default` unless the user keeps several).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyId {
    provider: String,
    account: String,
}

fn valid_part(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

impl KeyId {
    pub const DEFAULT_ACCOUNT: &'static str = "default";

    pub fn new(provider: &str, account: &str) -> Result<Self, KeystoreError> {
        if !valid_part(provider) {
            return Err(KeystoreError::InvalidId {
                field: "provider",
                reason: "use 1-64 characters of a-z, 0-9, - or _",
            });
        }
        if !valid_part(account) {
            return Err(KeystoreError::InvalidId {
                field: "account",
                reason: "use 1-64 characters of a-z, 0-9, - or _",
            });
        }
        Ok(Self {
            provider: provider.to_owned(),
            account: account.to_owned(),
        })
    }

    pub fn for_provider(provider: &str) -> Result<Self, KeystoreError> {
        Self::new(provider, Self::DEFAULT_ACCOUNT)
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn account(&self) -> &str {
        &self.account
    }

    /// Service name used in OS stores.
    pub(crate) const SERVICE: &'static str = "work.fundaments.manang.agent";

    /// User name used in OS stores.
    pub(crate) fn entry_user(&self) -> String {
        format!("{}/{}", self.provider, self.account)
    }
}

/// Whether a key exists and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStatus {
    pub present: bool,
    /// The store holding the key; when absent, the store a new key would go to.
    pub store: StoreKind,
}

/// Longest key accepted.
pub const MAX_KEY_LEN: usize = 4096;

/// Checks and trims a pasted key (surrounding whitespace and newlines go).
pub(crate) fn clean_key(key: &str) -> Result<SecretString, KeystoreError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(KeystoreError::InvalidKey("the key is empty"));
    }
    if trimmed.len() > MAX_KEY_LEN {
        return Err(KeystoreError::InvalidKey("the key is too long"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(KeystoreError::InvalidKey(
            "the key contains control characters",
        ));
    }
    Ok(SecretString::from(trimmed.to_owned()))
}

/// A place keys can be kept.
pub trait KeyStore: Send + Sync {
    fn kind(&self) -> StoreKind;

    /// Saves (or replaces) a key.
    fn set(&self, id: &KeyId, key: &SecretString) -> Result<(), KeystoreError>;

    /// Reads a key. For the provider layer only: never return this to the UI.
    fn get(&self, id: &KeyId) -> Result<Option<SecretString>, KeystoreError>;

    /// Removes a key; true if one existed.
    fn delete(&self, id: &KeyId) -> Result<bool, KeystoreError>;

    fn status(&self, id: &KeyId) -> Result<KeyStatus, KeystoreError> {
        Ok(KeyStatus {
            present: self.get(id)?.is_some(),
            store: self.kind(),
        })
    }
}
