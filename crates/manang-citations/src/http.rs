use crate::error::CitationsError;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

pub const DEFAULT_ALLOWLIST: &[&str] =
    &["api.crossref.org", "api.openalex.org", "export.arxiv.org"];

pub const DEFAULT_USER_AGENT: &str =
    "Manang-Citations/0.1.0 (https://fundaments.work/manang; mailto:dev@manang.org)";

pub const DEFAULT_MAX_BODY_BYTES: usize = 2 * 1024 * 1024; // 2 MB
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_REDIRECTS: usize = 5;

/// An HTTP response representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn ok(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: HashMap::new(),
            body,
        }
    }

    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_default();
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn text(status: u16, text: &str) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: text.as_bytes().to_vec(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }
}

/// Abstract HTTP client interface allowing test mocks and sandboxing.
pub trait HttpClient: Send + Sync {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, CitationsError>;
}

/// A fake in-memory HTTP client for deterministic offline testing.
#[derive(Default, Clone)]
pub struct MockHttpClient {
    responses: Arc<RwLock<HashMap<String, HttpResponse>>>,
    requested_urls: Arc<RwLock<Vec<String>>>,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_response(&self, url: impl Into<String>, response: HttpResponse) {
        self.responses.write().insert(url.into(), response);
    }

    pub fn set_json(&self, url: impl Into<String>, status: u16, value: &serde_json::Value) {
        self.set_response(url, HttpResponse::json(status, value));
    }

    pub fn set_text(&self, url: impl Into<String>, status: u16, text: &str) {
        self.set_response(url, HttpResponse::text(status, text));
    }

    pub fn requests(&self) -> Vec<String> {
        self.requested_urls.read().clone()
    }
}

impl HttpClient for MockHttpClient {
    fn get(&self, url: &str, _headers: &[(&str, &str)]) -> Result<HttpResponse, CitationsError> {
        self.requested_urls.write().push(url.to_string());
        if let Some(resp) = self.responses.read().get(url) {
            Ok(resp.clone())
        } else {
            Ok(HttpResponse::not_found())
        }
    }
}

/// A blocking, sandboxed HTTP client with exact-host allowlisting and on-disk caching.
pub struct BlockingHttpClient {
    allowlist: HashSet<String>,
    user_agent: String,
    max_body_bytes: usize,
    timeout: Duration,
    max_redirects: usize,
    cache_dir: Option<PathBuf>,
}

impl Default for BlockingHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockingHttpClient {
    pub fn new() -> Self {
        let allowlist: HashSet<String> = DEFAULT_ALLOWLIST
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();

        Self {
            allowlist,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            timeout: DEFAULT_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            cache_dir: None,
        }
    }

    pub fn with_cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    fn check_host_allowed(&self, host: &str) -> Result<(), CitationsError> {
        let lower = host.to_ascii_lowercase();
        if self.allowlist.contains(&lower) {
            Ok(())
        } else {
            Err(CitationsError::Http(format!(
                "host `{host}` is not in citations network allowlist"
            )))
        }
    }

    fn cache_key(url: &str) -> String {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn read_from_cache(&self, url: &str) -> Option<HttpResponse> {
        let cache_dir = self.cache_dir.as_ref()?;
        let path = cache_dir.join(Self::cache_key(url));
        if path.exists()
            && let Ok(bytes) = std::fs::read(path)
        {
            return Some(HttpResponse {
                status: 200,
                headers: HashMap::new(),
                body: bytes,
            });
        }
        None
    }

    fn write_to_cache(&self, url: &str, body: &[u8]) {
        if let Some(cache_dir) = &self.cache_dir {
            let _ = std::fs::create_dir_all(cache_dir);
            let path = cache_dir.join(Self::cache_key(url));
            let _ = std::fs::write(path, body);
        }
    }
}

impl HttpClient for BlockingHttpClient {
    fn get(
        &self,
        initial_url: &str,
        extra_headers: &[(&str, &str)],
    ) -> Result<HttpResponse, CitationsError> {
        // Check disk cache first
        if let Some(cached) = self.read_from_cache(initial_url) {
            return Ok(cached);
        }

        let mut current_url = initial_url.to_string();

        for _hop in 0..=self.max_redirects {
            let parsed = Url::parse(&current_url)
                .map_err(|e| CitationsError::Http(format!("invalid URL `{current_url}`: {e}")))?;

            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err(CitationsError::Http(format!(
                    "unsupported scheme `{}` in `{current_url}`",
                    parsed.scheme()
                )));
            }

            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(CitationsError::Http(
                    "URLs with embedded credentials are not permitted".to_string(),
                ));
            }

            let host = parsed
                .host_str()
                .ok_or_else(|| CitationsError::Http(format!("missing host in `{current_url}`")))?;
            self.check_host_allowed(host)?;

            // Build ureq Agent with 0 redirects so we control each hop
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .max_redirects(0)
                .http_status_as_error(false)
                .proxy(None)
                .timeout_global(Some(self.timeout))
                .user_agent(&self.user_agent)
                .build()
                .into();

            let mut req_builder = agent.get(&current_url);
            for (name, val) in extra_headers {
                req_builder = req_builder.header(*name, *val);
            }

            let mut response = req_builder
                .call()
                .map_err(|e| CitationsError::Http(format!("HTTP request failed: {e}")))?;

            let status = response.status().as_u16();

            // Handle redirect codes
            if matches!(status, 301 | 302 | 303 | 307 | 308)
                && let Some(loc_header) = response.headers().get("location")
                && let Ok(loc_str) = loc_header.to_str()
            {
                let next_url = parsed.join(loc_str).map_err(|e| {
                    CitationsError::Http(format!("failed to resolve redirect: {e}"))
                })?;
                current_url = next_url.to_string();
                continue;
            }

            // Collect headers
            let mut headers = HashMap::new();
            for (name, val) in response.headers() {
                if let Ok(v) = val.to_str() {
                    headers.insert(name.as_str().to_ascii_lowercase(), v.to_string());
                }
            }

            // Read response body with limit
            let body = response
                .body_mut()
                .with_config()
                .limit(self.max_body_bytes as u64)
                .read_to_vec()
                .map_err(|e| CitationsError::Http(format!("failed reading response body: {e}")))?;

            if status == 200 {
                self.write_to_cache(initial_url, &body);
            }

            return Ok(HttpResponse {
                status,
                headers,
                body,
            });
        }

        Err(CitationsError::Http(format!(
            "exceeded max redirects ({})",
            self.max_redirects
        )))
    }
}
