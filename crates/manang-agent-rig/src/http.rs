//! The HTTP seam under Rig: our retry, offline and audit rules as a Rig `HttpClientExt`.
//!
//! [`PolicyHttp`] wraps any Rig HTTP client. It enforces the loopback-only rule on IP-literal
//! URLs, retries transient failures (our `retry.rs` rules) before any stream data was delivered,
//! and never follows redirects (that is the inner client's job: ours is built with
//! `redirect::Policy::none()`). [`RequestTap`] sees, and may rewrite, the exact final request
//! body and headers; it runs once per logical request, before the retry loop.

use crate::retry::{RetryPolicy, parse_retry_after};
use bytes::Bytes;
use rig_core::http_client::{
    DynHttpClient, Error, HttpClientExt, HttpMiddleware, LazyBody, MultipartForm, Request,
    Response, Result, StreamingResponse,
};
use rig_core::wasm_compat::{WasmBoxedFuture, WasmCompatSend};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use url::{Host, Url};

/// A request refused by policy before it left the machine. Reaches the caller wrapped in
/// Rig's `Error::Instance` and is mapped to the `offline_blocked` error code.
#[derive(Debug, Clone, thiserror::Error)]
#[error("blocked: {0}")]
pub struct Blocked(pub String);

/// Checks an IP-literal host without touching DNS; hostnames pass here (the resolver of the
/// native client checks them at connect time, see `native::LoopbackResolver`).
pub fn ensure_loopback_literal(url: &str) -> std::result::Result<(), String> {
    let url = Url::parse(url).map_err(|_| "invalid URL".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http(s) URLs are allowed".to_owned());
    }
    let blocked = || "offline mode allows only loopback addresses, not this address".to_owned();
    match url.host() {
        Some(Host::Ipv4(ip)) if !ip.is_loopback() => Err(blocked()),
        Some(Host::Ipv6(ip)) if !is_loopback(IpAddr::V6(ip)) => Err(blocked()),
        Some(_) => Ok(()),
        None => Err("URL has no host".to_owned()),
    }
}

/// Whether an address is loopback (IPv4-mapped IPv6 loopback included).
pub fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// Sees every outgoing request (headers with secrets masked, final body) and may rewrite the
/// body. Used for the audit log and for redaction.
pub trait RequestTap: Send + Sync {
    /// `headers` has values of `authorization`, `x-api-key` and `api-key` replaced by `***`.
    /// Return `Some(new_body)` to send a different body (redaction).
    fn on_request(&self, url: &str, headers: &[(String, String)], body: &[u8]) -> Option<Vec<u8>>;
}

const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "api-key",
    "proxy-authorization",
];

struct TapMiddleware(Arc<dyn RequestTap>);

impl HttpMiddleware for TapMiddleware {
    fn before_request_body<'a>(
        &'a self,
        _method: &'a http::Method,
        uri: &'a http::Uri,
        headers: &'a http::HeaderMap,
        body: Bytes,
    ) -> WasmBoxedFuture<'a, Result<Bytes>> {
        Box::pin(async move {
            let masked: Vec<(String, String)> = headers
                .iter()
                .map(|(name, value)| {
                    let shown = if SECRET_HEADERS.contains(&name.as_str()) {
                        "***".to_owned()
                    } else {
                        value.to_str().unwrap_or("<binary>").to_owned()
                    };
                    (name.as_str().to_owned(), shown)
                })
                .collect();
            match self.0.on_request(&uri.to_string(), &masked, &body) {
                Some(rewritten) => Ok(Bytes::from(rewritten)),
                None => Ok(body),
            }
        })
    }
}

/// Settings of the policy layer.
#[derive(Clone, Default)]
pub struct HttpPolicy {
    pub retry: RetryPolicy,
    /// Refuse IP-literal destinations that are not loopback. (Hostnames are checked by the
    /// native client's resolver, see [`crate::native`].)
    pub loopback_only: bool,
    pub tap: Option<Arc<dyn RequestTap>>,
}

impl std::fmt::Debug for HttpPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpPolicy")
            .field("retry", &self.retry)
            .field("loopback_only", &self.loopback_only)
            .field("tap", &self.tap.is_some())
            .finish()
    }
}

/// Rig's HTTP client trait with our policy around an inner client.
#[derive(Clone)]
pub struct PolicyHttp {
    inner: DynHttpClient,
    retry: RetryPolicy,
    loopback_only: bool,
}

impl PolicyHttp {
    /// Wraps `inner`.
    pub fn new(inner: impl HttpClientExt + 'static, policy: &HttpPolicy) -> Self {
        Self {
            inner: DynHttpClient::new(inner),
            retry: policy.retry,
            loopback_only: policy.loopback_only,
        }
    }

    /// The client to hand to Rig's `connect`: the policy layer, with the tap (if any) as Rig
    /// middleware in front of it.
    pub fn into_dyn(self, policy: &HttpPolicy) -> DynHttpClient {
        let dynamic = DynHttpClient::new(self);
        match &policy.tap {
            Some(tap) => dynamic.with_middleware(TapMiddleware(Arc::clone(tap))),
            None => dynamic,
        }
    }

    fn check(&self, uri: &http::Uri) -> Result<()> {
        if self.loopback_only {
            ensure_loopback_literal(&uri.to_string())
                .map_err(|reason| Error::instance(Blocked(reason)))?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for PolicyHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PolicyHttp")
    }
}

fn rebuild(parts: &http::request::Parts, body: &Bytes) -> Result<Request<Bytes>> {
    let mut builder = Request::builder()
        .method(parts.method.clone())
        .uri(parts.uri.clone());
    if let Some(headers) = builder.headers_mut() {
        *headers = parts.headers.clone();
    }
    builder.body(body.clone()).map_err(Error::Protocol)
}

impl HttpClientExt for PolicyHttp {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        T: Into<Bytes>,
        T: WasmCompatSend,
        U: From<Bytes>,
        U: WasmCompatSend + 'static,
    {
        // Non-streaming calls (model listing, key check) get the loopback rule but no retry.
        let checked = self.check(req.uri());
        let call = checked.map(|()| self.inner.send::<T, U>(req));
        async move { call?.await }
    }

    fn send_multipart<U>(
        &self,
        req: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        U: From<Bytes>,
        U: WasmCompatSend + 'static,
    {
        let checked = self.check(req.uri());
        let call = checked.map(|()| self.inner.send_multipart::<U>(req));
        async move { call?.await }
    }

    fn send_streaming<T>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse>> + WasmCompatSend
    where
        T: Into<Bytes> + WasmCompatSend,
    {
        let this = self.clone();
        let (parts, body) = req.map(Into::into).into_parts();
        async move {
            this.check(&parts.uri)?;
            let mut retry = 0u32;
            loop {
                let attempt = rebuild(&parts, &body)?;
                match this.inner.send_streaming(attempt).await {
                    Ok(response) => return Ok(response),
                    Err(error) => {
                        retry += 1;
                        match wait_for(&this.retry, &error, retry) {
                            Some(wait) => futures_timer::Delay::new(wait).await,
                            None => return Err(error),
                        }
                    }
                }
            }
        }
    }
}

/// How long to wait before the next attempt, or `None` to give up.
fn wait_for(policy: &RetryPolicy, error: &Error, retry: u32) -> Option<Duration> {
    if retry >= policy.max_attempts {
        return None;
    }
    let status = error.non_success_status()?.as_u16();
    let retry_after = error
        .non_success_headers()
        .and_then(|h| h.get(http::header::RETRY_AFTER))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_retry_after(Some(v)));
    policy.delay_for(status, retry_after, retry)
}
