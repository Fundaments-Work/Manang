//! The native HTTP client: our `reqwest::Client` (rustls with ring, no ambient proxy, no
//! redirects, optional loopback-only DNS) handed to Rig through `rig-reqwest`.
//!
//! Not compiled for wasm32: a browser build supplies its own `HttpClientExt` (Rig ships a fetch
//! client) and wraps it in [`crate::PolicyHttp`].

use crate::http::{HttpPolicy, PolicyHttp, is_loopback};
use crate::provider::Http;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use rig_reqwest::ReqwestClient;
use std::net::SocketAddr;
use std::sync::Once;
use std::time::Duration;

/// Settings of the reqwest client.
#[derive(Debug, Clone)]
pub struct NativeConfig {
    /// Explicit proxy URL. Ambient `HTTP(S)_PROXY` variables are NOT read.
    pub proxy: Option<String>,
    pub connect_timeout: Duration,
    pub user_agent: String,
}

impl Default for NativeConfig {
    fn default() -> Self {
        Self {
            proxy: None,
            connect_timeout: Duration::from_secs(15),
            user_agent: concat!("Manang/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }
}

/// The client could not be configured.
#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid transport configuration: {0}")]
pub struct NativeError(String);

static PROVIDER_ONCE: Once = Once::new();

fn install_crypto_provider() {
    PROVIDER_ONCE.call_once(|| {
        // Err means another provider is already installed; that is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A DNS resolver that refuses non-loopback answers, so the offline rule also holds at connect
/// time (no resolve-then-rebind gap).
#[derive(Debug, Clone, Copy)]
struct LoopbackResolver;

impl Resolve for LoopbackResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .collect();
            if addrs.is_empty() || !addrs.iter().all(|a| is_loopback(a.ip())) {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                    "offline mode allows only loopback addresses",
                ));
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Builds the client Rig sends through: our reqwest client inside [`PolicyHttp`] (retry,
/// loopback literal check) with the policy's tap as middleware.
pub fn build_client(config: &NativeConfig, policy: &HttpPolicy) -> Result<Http, NativeError> {
    install_crypto_provider();
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(config.connect_timeout)
        .user_agent(&config.user_agent)
        .no_proxy();
    if let Some(proxy) = &config.proxy {
        let proxy =
            reqwest::Proxy::all(proxy).map_err(|_| NativeError("invalid proxy URL".to_owned()))?;
        builder = builder.proxy(proxy);
    }
    if policy.loopback_only {
        builder = builder.dns_resolver(LoopbackResolver);
    }
    let client = builder
        .build()
        .map_err(|e| NativeError(e.without_url().to_string()))?;
    Ok(Http::new(
        PolicyHttp::new(ReqwestClient::from(client), policy).into_dyn(policy),
    ))
}
