//! The API key wrapper: zeroised on drop, never printed.

use secrecy::{ExposeSecret, SecretString};

/// An API key. `Debug` prints `ApiKey(***)`; the value is only reachable
/// through [`ApiKey::expose`], which adapters call to build one header.
#[derive(Clone)]
pub struct ApiKey(SecretString);

impl ApiKey {
    pub fn new(key: &str) -> Self {
        Self(SecretString::from(key.to_owned()))
    }

    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(***)")
    }
}
