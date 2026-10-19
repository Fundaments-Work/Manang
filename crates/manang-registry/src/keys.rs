//! The registry's trust root: ed25519 public keys embedded in the app, and
//! verification of `index.json.sig` over the exact bytes of `index.json`.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, VerifyingKey};

use crate::error::RegistryError;

/// The keys compiled into this build (`registry-keys`).
const EMBEDDED: &str = include_str!("../registry-keys");

/// The keys an index signature may verify against.
#[derive(Debug, Clone, Default)]
pub struct TrustedKeys {
    keys: Vec<VerifyingKey>,
}

impl TrustedKeys {
    /// Parse a key list: `<id> <base64 public key>` per line, `#` comments.
    /// Keys whose id starts with `placeholder` are skipped: they stand in for
    /// a real key and must never be able to vouch for anything.
    pub fn parse(text: &str) -> Result<Self, RegistryError> {
        let mut keys = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (id, b64) = line
                .split_once(char::is_whitespace)
                .ok_or_else(|| RegistryError::BadIndex(format!("bad key line `{line}`")))?;
            if id.starts_with("placeholder") {
                continue;
            }
            let bytes = STANDARD
                .decode(b64.trim())
                .map_err(|_| RegistryError::BadIndex(format!("key `{id}` is not base64")))?;
            let array: [u8; 32] = bytes
                .try_into()
                .map_err(|_| RegistryError::BadIndex(format!("key `{id}` is not 32 bytes")))?;
            let key = VerifyingKey::from_bytes(&array)
                .map_err(|_| RegistryError::BadIndex(format!("key `{id}` is not a valid key")))?;
            keys.push(key);
        }
        Ok(Self { keys })
    }

    /// The keys embedded in this build. Empty while only the placeholder is
    /// there, so every index is then refused.
    pub fn embedded() -> Self {
        // The embedded file is ours and tested; a malformed line leaves the
        // list empty (nothing verifies) rather than trusting a partial list.
        Self::parse(EMBEDDED).unwrap_or_default()
    }

    /// Keys from raw public keys (tests, tools).
    pub fn from_keys(keys: Vec<VerifyingKey>) -> Self {
        Self { keys }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Check `signature_b64` (base64 of the 64-byte signature) over `bytes`.
    /// `verify_strict` also refuses weak (small-order) keys and malleable
    /// signatures.
    pub fn verify(&self, bytes: &[u8], signature_b64: &[u8]) -> Result<(), RegistryError> {
        let text = std::str::from_utf8(signature_b64).map_err(|_| RegistryError::Signature)?;
        let raw = STANDARD
            .decode(text.trim())
            .map_err(|_| RegistryError::Signature)?;
        let sig = Signature::from_slice(&raw).map_err(|_| RegistryError::Signature)?;
        if self
            .keys
            .iter()
            .any(|key| key.verify_strict(bytes, &sig).is_ok())
        {
            Ok(())
        } else {
            Err(RegistryError::Signature)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    pub(crate) fn test_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    pub(crate) fn sign(key: &SigningKey, bytes: &[u8]) -> String {
        STANDARD.encode(key.sign(bytes).to_bytes())
    }

    pub(crate) fn trusting(key: &SigningKey) -> TrustedKeys {
        TrustedKeys::from_keys(vec![key.verifying_key()])
    }

    #[test]
    fn the_embedded_list_parses_and_trusts_nothing_until_a_real_key_is_set() {
        assert!(TrustedKeys::parse(EMBEDDED).is_ok());
        assert!(TrustedKeys::embedded().is_empty());
    }

    #[test]
    fn a_good_signature_verifies_and_a_changed_byte_does_not() {
        let key = test_key(1);
        let keys = trusting(&key);
        let sig = sign(&key, b"index bytes");
        assert!(keys.verify(b"index bytes", sig.as_bytes()).is_ok());
        assert!(keys.verify(b"index bytez", sig.as_bytes()).is_err());
        assert!(keys.verify(b"index bytes", b"AAAA").is_err());
        assert!(keys.verify(b"index bytes", b"not base64 !!").is_err());
    }

    #[test]
    fn another_key_cannot_vouch_and_any_listed_key_can() {
        let (a, b) = (test_key(1), test_key(2));
        let sig = sign(&b, b"x");
        assert!(trusting(&a).verify(b"x", sig.as_bytes()).is_err());
        let both = TrustedKeys::from_keys(vec![a.verifying_key(), b.verifying_key()]);
        assert!(both.verify(b"x", sig.as_bytes()).is_ok());
    }

    #[test]
    fn key_lists_are_parsed_strictly() {
        let key = test_key(3);
        let line = format!("main {}", STANDARD.encode(key.verifying_key().to_bytes()));
        assert_eq!(TrustedKeys::parse(&line).expect("ok").keys.len(), 1);
        assert!(TrustedKeys::parse("main !!!").is_err());
        assert!(TrustedKeys::parse("main AAAA").is_err());
        assert!(TrustedKeys::parse("lonely").is_err());
    }
}
