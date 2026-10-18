//! The HTTP seam: a streaming POST behind a trait.
//!
//! Adapters only know [`HttpTransport`]; tests use an in-process server over
//! the real [`ReqwestTransport`], and a browser build can supply its own.

use crate::loopback::{LoopbackResolver, ensure_loopback_literal};
use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use std::sync::Once;
use std::time::Duration;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// A request to POST. Header values may hold secrets: `Debug` hides them.
#[derive(Clone)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("header_names", &names)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// A byte stream of a response body. Dropping it closes the connection.
pub type ByteStream = BoxStream<'static, Result<Bytes, TransportError>>;

/// A response whose body has not been read yet.
pub struct HttpResponse {
    pub status: u16,
    /// Header names are lowercase.
    pub headers: Vec<(String, String)>,
    pub body: ByteStream,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

impl std::fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Transport failures. Messages never contain header values.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TransportError {
    #[error("could not connect: {0}")]
    Connect(String),
    #[error("request timed out")]
    Timeout,
    #[error("connection failed mid-response: {0}")]
    Body(String),
    #[error("request was cancelled")]
    Cancelled,
    #[error("blocked: {0}")]
    Blocked(String),
    #[error("invalid transport configuration: {0}")]
    Config(String),
}

/// A streaming POST.
pub trait HttpTransport: Send + Sync {
    /// Sends the request and returns once the response headers arrive.
    /// Cancelling the token aborts the attempt.
    fn post_stream<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, TransportError>>;
}

impl std::fmt::Debug for dyn HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpTransport")
    }
}

/// Settings of the reqwest transport.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Explicit proxy URL. Ambient `HTTP(S)_PROXY` variables are NOT read:
    /// the policy layer supplies this value. Default: none.
    pub proxy: Option<String>,
    /// Allow only loopback destinations, checked on every DNS resolution.
    pub loopback_only: bool,
    pub connect_timeout: Duration,
    pub user_agent: String,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            proxy: None,
            loopback_only: false,
            connect_timeout: Duration::from_secs(15),
            user_agent: concat!("Manang/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }
}

/// The production transport: reqwest over rustls (ring). Redirects are never
/// followed (a redirect could leave the loopback or allowlisted host).
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
    loopback_only: bool,
}

static PROVIDER_ONCE: Once = Once::new();

fn install_crypto_provider() {
    PROVIDER_ONCE.call_once(|| {
        // Err means another provider is already installed; that is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

impl ReqwestTransport {
    pub fn new(config: &TransportConfig) -> Result<Self, TransportError> {
        install_crypto_provider();
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(config.connect_timeout)
            .user_agent(&config.user_agent)
            .no_proxy();
        if let Some(proxy) = &config.proxy {
            let proxy = reqwest::Proxy::all(proxy)
                .map_err(|_| TransportError::Config("invalid proxy URL".to_owned()))?;
            builder = builder.proxy(proxy);
        }
        if config.loopback_only {
            builder = builder.dns_resolver(LoopbackResolver);
        }
        let client = builder
            .build()
            .map_err(|e| TransportError::Config(e.to_string()))?;
        Ok(Self {
            client,
            loopback_only: config.loopback_only,
        })
    }

    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        if self.loopback_only {
            // IP literals bypass the resolver, so check them here.
            ensure_loopback_literal(&request.url).map_err(TransportError::Blocked)?;
        }
        let mut builder = self.client.post(&request.url).body(request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder.send().await.map_err(map_send_error)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|v| (k.as_str().to_ascii_lowercase(), v.to_owned()))
            })
            .collect();
        let body = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| TransportError::Body(strip_url(e))))
            .boxed();
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn strip_url(error: reqwest::Error) -> String {
    // reqwest errors embed the URL; it can carry a key in a query string.
    error.without_url().to_string()
}

fn map_send_error(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else if error.is_connect() {
        TransportError::Connect(strip_url(error))
    } else {
        TransportError::Body(strip_url(error))
    }
}

impl HttpTransport for ReqwestTransport {
    fn post_stream<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            tokio::select! {
                () = cancel.cancelled() => Err(TransportError::Cancelled),
                result = self.send(request) => result,
            }
        })
    }
}
