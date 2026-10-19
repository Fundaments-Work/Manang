//! Where the registry lives and how bytes reach the app: the base URL rules
//! and the [`Transport`] trait (tests use a local server; the browser build
//! will use `fetch`).

use std::io::Read;

use crate::error::RegistryError;

/// Where registry files are served from, taken from `MANANG_REGISTRY_URL` at
/// build time. The default is a placeholder that never resolves, so a build
/// that forgot to set it cannot fetch from anywhere.
pub const DEFAULT_URL: &str = match option_env!("MANANG_REGISTRY_URL") {
    Some(url) => url,
    None => PLACEHOLDER_URL,
};

/// The default of [`DEFAULT_URL`]: reserved `.invalid` TLD, never reachable.
pub const PLACEHOLDER_URL: &str = "https://registry.manang.invalid";

/// A validated registry base URL: https (plain http only for loopback, for
/// tests and local mirrors), no query, no trailing slash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    url: String,
    host: String,
}

impl Base {
    pub fn parse(url: &str) -> Result<Self, RegistryError> {
        let url = url.trim().trim_end_matches('/');
        let rest = url
            .strip_prefix("https://")
            .map(|r| (r, true))
            .or_else(|| url.strip_prefix("http://").map(|r| (r, false)));
        let Some((rest, https)) = rest else {
            return Err(RegistryError::InsecureUrl);
        };
        if rest.contains(['?', '#', '@', ' ']) || rest.is_empty() {
            return Err(RegistryError::InsecureUrl);
        }
        let host = rest.split('/').next().unwrap_or_default().to_owned();
        let bare = host.rsplit_once(':').map_or(host.as_str(), |(h, port)| {
            if port.bytes().all(|b| b.is_ascii_digit()) {
                h
            } else {
                host.as_str()
            }
        });
        if bare.is_empty() {
            return Err(RegistryError::InsecureUrl);
        }
        if bare.ends_with(".invalid") {
            return Err(RegistryError::NotConfigured);
        }
        let loopback = bare == "127.0.0.1" || bare == "localhost" || bare == "[::1]";
        if !https && !loopback {
            return Err(RegistryError::InsecureUrl);
        }
        Ok(Self {
            url: url.to_owned(),
            host,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// `host[:port]`, for messages and the consent text.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// `relative` (already validated as a safe path) below the base.
    pub fn join(&self, relative: &str) -> String {
        format!("{}/{}", self.url, relative.trim_start_matches('/'))
    }
}

/// One HTTP answer.
pub struct Response {
    pub status: u16,
    pub etag: Option<String>,
    pub body: Box<dyn Read + Send>,
}

/// Fetches a URL. `if_none_match` makes the request conditional. Network
/// failures are [`RegistryError::Network`]; HTTP statuses are returned, not
/// raised, so the caller can treat 304 as success.
pub trait Transport: Send + Sync {
    fn get(&self, url: &str, if_none_match: Option<&str>) -> Result<Response, RegistryError>;
}

/// The production transport: ureq with rustls on ring.
#[cfg(feature = "native")]
#[derive(Debug, Clone, Copy, Default)]
pub struct HttpTransport;

#[cfg(feature = "native")]
impl Transport for HttpTransport {
    fn get(&self, url: &str, if_none_match: Option<&str>) -> Result<Response, RegistryError> {
        use std::time::Duration;

        let host = url
            .split("://")
            .nth(1)
            .and_then(|r| r.split('/').next())
            .unwrap_or_default()
            .to_owned();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(3)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(10 * 60)))
            .user_agent("manang-registry")
            .build()
            .into();
        let mut request = agent.get(url);
        if let Some(etag) = if_none_match {
            request = request.header("If-None-Match", etag);
        }
        let response = request.call().map_err(|e| RegistryError::Network {
            host,
            detail: e.to_string(),
        })?;
        let status = response.status().as_u16();
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        Ok(Response {
            status,
            etag,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_is_required_except_for_loopback() {
        assert!(Base::parse("https://registry.example.org/").is_ok());
        assert!(matches!(
            Base::parse("http://registry.example.org"),
            Err(RegistryError::InsecureUrl)
        ));
        assert!(Base::parse("http://127.0.0.1:8080").is_ok());
        assert!(matches!(
            Base::parse("ftp://x.example"),
            Err(RegistryError::InsecureUrl)
        ));
        assert!(Base::parse("https://user@host.example").is_err());
        assert!(Base::parse("https://host.example/?x=1").is_err());
    }

    #[test]
    fn the_placeholder_is_not_available() {
        assert!(matches!(
            Base::parse(PLACEHOLDER_URL),
            Err(RegistryError::NotConfigured)
        ));
    }

    #[test]
    fn join_keeps_the_prefix_and_trims_slashes() {
        let base = Base::parse("https://example.org/registry/").expect("ok");
        assert_eq!(
            base.join("index.json"),
            "https://example.org/registry/index.json"
        );
        assert_eq!(base.host(), "example.org");
    }
}
