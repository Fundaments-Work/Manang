//! Building a provider from the settings, the preset and the key.
//!
//! The default engine is Rig behind `manang-agent-rig`; the in-house adapters of
//! `manang-agent-providers` are the selectable fallback (`agent.client = builtin`). Both sit
//! behind `manang_agent_core::Provider`, so nothing else in this crate knows which one runs.
//! Tests substitute a [`ProviderFactory`] that replays cassettes.

use crate::error::AgentError;
use crate::settings::Engine;
use manang_agent_core::{ChatRequest, Provider, ProviderKind, ProviderPreset};
use manang_agent_providers as builtin;
use manang_agent_rig as rig;
use manang_keystore::{ExposeSecret, SecretString};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Everything needed to talk to one provider. `Debug` hides the key.
#[derive(Debug, Clone)]
pub struct ProviderSpec {
    pub preset: ProviderPreset,
    pub base_url: String,
    pub api_key: Option<SecretString>,
    pub offline: bool,
    pub engine: Engine,
    /// `reasoning_effort` for providers that take it (DeepSeek), fixed per session.
    pub effort: Option<String>,
}

/// The request as it would go on the wire. Header values of keys are masked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WirePreview {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The JSON body, exactly as sent.
    pub body: String,
}

pub type PreviewFuture = Pin<Box<dyn Future<Output = Result<WirePreview, AgentError>> + Send>>;

/// Makes providers. The shell uses [`RealFactory`].
pub trait ProviderFactory: Send + Sync {
    fn build(&self, spec: &ProviderSpec) -> Result<Arc<dyn Provider>, AgentError>;

    /// The exact request `build(spec)` would send for `request`. Sends nothing.
    fn preview(&self, spec: &ProviderSpec, request: ChatRequest) -> PreviewFuture;
}

/// The production factory.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFactory;

fn rig_key(spec: &ProviderSpec) -> Option<rig::ApiKey> {
    spec.api_key
        .as_ref()
        .map(|k| rig::ApiKey::new(k.expose_secret()))
}

fn builtin_key(spec: &ProviderSpec) -> Option<builtin::ApiKey> {
    spec.api_key
        .as_ref()
        .map(|k| builtin::ApiKey::new(k.expose_secret()))
}

fn rig_provider(spec: &ProviderSpec) -> Result<rig::RigProvider, AgentError> {
    let policy = rig::HttpPolicy {
        loopback_only: spec.offline,
        ..rig::HttpPolicy::default()
    };
    let http = rig::native::build_client(&rig::native::NativeConfig::default(), &policy)
        .map_err(|e| AgentError::new("provider_setup", e.to_string()))?;
    // The key header of a preset without a key is simply absent.
    let mut backend = rig::Backend::from_preset(&spec.preset, Some(&spec.base_url), rig_key(spec))
        .map_err(|m| AgentError::new("provider_setup", m))?;
    if let (rig::Backend::OpenAiCompat(settings), Some(effort)) = (&mut backend, &spec.effort)
        && settings.reasoning_passback
    {
        settings
            .extra_body
            .insert("reasoning_effort".to_owned(), serde_json::json!(effort));
    }
    Ok(rig::RigProvider::new(backend, http))
}

fn builtin_transport(spec: &ProviderSpec) -> Result<Arc<dyn builtin::HttpTransport>, AgentError> {
    let config = builtin::TransportConfig {
        loopback_only: spec.offline,
        ..builtin::TransportConfig::default()
    };
    Ok(Arc::new(builtin::ReqwestTransport::new(&config).map_err(
        |e| AgentError::new("provider_setup", e.to_string()),
    )?))
}

enum Builtin {
    Anthropic(builtin::AnthropicProvider),
    OpenAi(builtin::OpenAiCompatProvider),
}

fn builtin_provider(spec: &ProviderSpec) -> Result<Builtin, AgentError> {
    let transport = builtin_transport(spec)?;
    match spec.preset.kind {
        ProviderKind::Anthropic => {
            let key = builtin_key(spec)
                .ok_or_else(|| AgentError::new("key_missing", "this provider needs an API key"))?;
            let mut config = builtin::AnthropicConfig::new(key);
            config.base_url.clone_from(&spec.base_url);
            config.offline = spec.offline;
            Ok(Builtin::Anthropic(builtin::AnthropicProvider::new(
                config, transport,
            )))
        }
        ProviderKind::OpenAiCompat => {
            let mut config = builtin::OpenAiCompatConfig::from_preset(
                &spec.preset,
                Some(&spec.base_url),
                builtin_key(spec),
            )
            .map_err(|m| AgentError::new("provider_setup", m))?;
            config.offline = spec.offline;
            if let Some(effort) = &spec.effort
                && config.reasoning_passback
            {
                config
                    .extra_body
                    .insert("reasoning_effort".to_owned(), serde_json::json!(effort));
            }
            Ok(Builtin::OpenAi(builtin::OpenAiCompatProvider::new(
                config, transport,
            )))
        }
    }
}

impl ProviderFactory for RealFactory {
    fn build(&self, spec: &ProviderSpec) -> Result<Arc<dyn Provider>, AgentError> {
        match spec.engine {
            Engine::Rig => Ok(Arc::new(rig_provider(spec)?)),
            Engine::Builtin => Ok(match builtin_provider(spec)? {
                Builtin::Anthropic(p) => Arc::new(p),
                Builtin::OpenAi(p) => Arc::new(p),
            }),
        }
    }

    fn preview(&self, spec: &ProviderSpec, request: ChatRequest) -> PreviewFuture {
        let spec = spec.clone();
        Box::pin(async move {
            match spec.engine {
                Engine::Rig => {
                    let provider = rig_provider(&spec)?;
                    let preview = provider
                        .preview(&request)
                        .await
                        .map_err(|m| AgentError::new("provider_setup", m))?;
                    Ok(WirePreview {
                        url: preview.url,
                        headers: preview.headers,
                        body: String::from_utf8_lossy(&preview.body).into_owned(),
                    })
                }
                Engine::Builtin => {
                    let (url, body) = match builtin_provider(&spec)? {
                        Builtin::Anthropic(p) => (
                            format!("{}/v1/messages", spec.base_url.trim_end_matches('/')),
                            p.request_body(&request),
                        ),
                        Builtin::OpenAi(p) => (
                            format!("{}/chat/completions", spec.base_url.trim_end_matches('/')),
                            p.request_body(&request),
                        ),
                    };
                    let body = body?;
                    Ok(WirePreview {
                        url,
                        headers: vec![("authorization".to_owned(), "***".to_owned())],
                        body: String::from_utf8_lossy(&body).into_owned(),
                    })
                }
            }
        })
    }
}

/// A user-facing explanation of a provider error code, for "Test connection" and error lines.
pub fn friendly(code: &str, message: &str) -> String {
    match code {
        "authentication_error" | "http_401" => {
            "The provider rejected the key. Check that you pasted the whole key and that it \
             belongs to this provider."
                .to_owned()
        }
        "insufficient_balance" | "http_402" => {
            "The provider says the account has no balance. Add funds at the provider's site."
                .to_owned()
        }
        "http_404" => {
            "The provider does not know this model or address. Check the model name and the base \
             URL."
                .to_owned()
        }
        "http_429" | "rate_limit_error" => {
            "The provider is rate limiting this key. Wait a moment and try again.".to_owned()
        }
        "connect_failed" => {
            "Could not reach the provider. Check your connection and the base URL.".to_owned()
        }
        "offline_blocked" => "Offline mode only allows providers on this machine.".to_owned(),
        _ => message.to_owned(),
    }
}
