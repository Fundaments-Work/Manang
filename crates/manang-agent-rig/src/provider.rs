//! `RigProvider`: our `Provider` trait over Rig's `Model::stream`.

use crate::decode::drive;
use crate::encode::{Wire, encode};
use manang_agent_core::{
    ChatRequest, EventStream, MaxTokensField, Provider, ProviderError, ProviderEvent, ProviderKind,
    ProviderPreset,
};
use rig_core::Model;
use rig_core::completion::CompletionRequest;
use rig_core::http_client::{
    DynHttpClient, Error as HttpError, HttpClientExt, LazyBody, MultipartForm, Request, Response,
    StreamingResponse,
};
use rig_core::message::Origin;
use rig_core::providers::anthropic::{AnthropicConfig, ToolInputStreaming};
use rig_core::providers::openai::OpenAIConfig;
use rig_core::providers::openai::wire::{Auth, DEEPSEEK, Dialect, OPENAI};
use rig_core::streaming::CompletionStream;
use rig_core::wasm_compat::WasmCompatSend;
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex, PoisonError};

/// An API key. `Debug` never shows it.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: &str) -> Self {
        Self(key.to_owned())
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(***)")
    }
}

/// The HTTP client a provider sends through (see [`crate::native::build_client`]).
#[derive(Clone, Debug)]
pub struct Http(pub(crate) DynHttpClient);

impl Http {
    /// Wraps a policy-wrapped Rig client. Used by the native builder and by browser builds.
    pub(crate) fn new(client: DynHttpClient) -> Self {
        Self(client)
    }
}

/// Settings for Anthropic Messages.
#[derive(Debug, Clone)]
pub struct AnthropicSettings {
    pub base_url: String,
    pub api_key: ApiKey,
    pub version: String,
    /// Ask Anthropic to stream tool input as it is written (`eager_input_streaming` on every
    /// tool). Off by default so request bodies match the in-house adapter.
    pub eager_tool_input: bool,
}

impl AnthropicSettings {
    pub fn new(api_key: ApiKey) -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_owned(),
            api_key,
            version: "2023-06-01".to_owned(),
            eager_tool_input: false,
        }
    }
}

/// Settings for an OpenAI-compatible Chat Completions server.
#[derive(Debug, Clone)]
pub struct OpenAiSettings {
    /// Includes the version path, e.g. `http://localhost:11434/v1`.
    pub base_url: String,
    /// `None` sends no key header.
    pub api_key: Option<ApiKey>,
    /// Header that carries the key; `None` means `Authorization: Bearer <key>`. Only
    /// `api-key` is supported besides the default.
    pub key_header: Option<String>,
    pub max_tokens_field: MaxTokensField,
    /// Send the model's reasoning back on assistant messages (DeepSeek).
    pub reasoning_passback: bool,
    /// Extra top-level request fields.
    pub extra_body: Map<String, Value>,
}

impl OpenAiSettings {
    pub fn new(base_url: &str, api_key: Option<ApiKey>) -> Self {
        Self {
            base_url: base_url.to_owned(),
            api_key,
            key_header: None,
            max_tokens_field: MaxTokensField::MaxTokens,
            reasoning_passback: false,
            extra_body: Map::new(),
        }
    }
}

/// Which provider this talks to.
#[derive(Debug, Clone)]
pub enum Backend {
    Anthropic(AnthropicSettings),
    OpenAiCompat(OpenAiSettings),
}

impl Backend {
    /// Builds the backend for a preset (`base_url` overrides the preset's URL).
    pub fn from_preset(
        preset: &ProviderPreset,
        base_url: Option<&str>,
        api_key: Option<ApiKey>,
    ) -> Result<Self, String> {
        match preset.kind {
            ProviderKind::Anthropic => {
                let key = api_key.ok_or_else(|| "this preset needs an API key".to_owned())?;
                let mut settings = AnthropicSettings::new(key);
                if let Some(url) = base_url.or(preset.base_url.as_deref()) {
                    settings.base_url = url.to_owned();
                }
                Ok(Self::Anthropic(settings))
            }
            ProviderKind::OpenAiCompat => {
                let url = base_url
                    .or(preset.base_url.as_deref())
                    .ok_or_else(|| "this preset needs a base URL".to_owned())?;
                let mut settings = OpenAiSettings::new(url, api_key);
                settings.key_header.clone_from(&preset.key_header);
                settings.max_tokens_field = preset.max_tokens_field;
                settings.reasoning_passback = preset.reasoning_passback;
                Ok(Self::OpenAiCompat(settings))
            }
        }
    }

    fn wire(&self) -> Wire {
        match self {
            Self::Anthropic(_) => Wire::Anthropic,
            Self::OpenAiCompat(_) => Wire::OpenAiCompat,
        }
    }
}

/// A provider backed by Rig.
#[derive(Clone, Debug)]
pub struct RigProvider {
    backend: Backend,
    http: Http,
}

/// The request as it would go on the wire (secrets masked).
#[derive(Debug, Clone)]
pub struct Preview {
    pub url: String,
    /// Header values of `x-api-key` and `authorization` are `***`.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RigProvider {
    pub fn new(backend: Backend, http: Http) -> Self {
        Self { backend, http }
    }

    fn open(
        &self,
        request: &ChatRequest,
        http: DynHttpClient,
    ) -> Result<(CompletionStream, manang_agent_core::ToolNameMap), String> {
        let wire = self.backend.wire();
        let empty = Map::new();
        let extra = match &self.backend {
            Backend::OpenAiCompat(s) => &s.extra_body,
            Backend::Anthropic(_) => &empty,
        };
        let (api, provider, reasoning_field) = match &self.backend {
            Backend::Anthropic(_) => ("anthropic.messages", "anthropic", None),
            Backend::OpenAiCompat(s) => (
                "openai.chat",
                openai_dialect(s).name,
                s.reasoning_passback.then_some("reasoning_content"),
            ),
        };
        let origin = Origin::new(api, provider, request.model.as_str());
        let encoded =
            encode(request, wire, &origin, reasoning_field, extra).map_err(|e| e.to_string())?;
        let names = encoded.names;
        let streamed = self
            .stream_with(
                &request.model,
                encoded.request,
                encoded.prompt_caching,
                http,
            )
            .map_err(|e| e.to_string())?;
        Ok((streamed, names))
    }

    fn stream_with(
        &self,
        model: &str,
        request: CompletionRequest,
        prompt_caching: bool,
        http: DynHttpClient,
    ) -> Result<CompletionStream, rig_core::ProviderError> {
        match &self.backend {
            Backend::Anthropic(s) => {
                let config = AnthropicConfig::new(s.api_key.0.as_str())
                    .with_base_url(&s.base_url)
                    .with_version(&s.version);
                let mut messages = config.connect(http).completion(model);
                messages.wire = messages
                    .wire
                    .with_tool_input_streaming(if s.eager_tool_input {
                        ToolInputStreaming::Eager
                    } else {
                        ToolInputStreaming::Off
                    });
                if prompt_caching {
                    messages.wire = messages.wire.with_prompt_caching();
                }
                Model::stream(&messages, request)
            }
            Backend::OpenAiCompat(s) => {
                let dialect = openai_dialect(s);
                let key = s.api_key.as_ref().map_or("", |k| k.0.as_str());
                let mut config = OpenAIConfig::with_key(&dialect, key).with_base_url(&s.base_url);
                if s.api_key.is_none() {
                    config.auth = Auth::OptionalBearer;
                }
                if s.key_header.as_deref() == Some("api-key") {
                    config.auth = Auth::ApiKeyHeader;
                }
                let chat = config.connect(http).chat(model);
                Model::stream(&chat, request)
            }
        }
    }

    /// The exact request `stream` would send: URL, headers (secrets masked) and JSON body.
    /// Nothing leaves the machine.
    pub async fn preview(&self, request: &ChatRequest) -> Result<Preview, String> {
        let capture = Capture::default();
        let (mut streamed, _) = self.open(request, DynHttpClient::new(capture.clone()))?;
        // Polling opens the (capturing) transport; its error ends the stream.
        let _ = futures_util::StreamExt::next(&mut streamed).await;
        capture
            .take()
            .ok_or_else(|| "no request was produced".to_owned())
    }
}

/// The OpenAI-shaped dialect: DeepSeek's when reasoning must be passed back, else a generic
/// gateway (optional bearer, `x-request-id`).
fn openai_dialect(settings: &OpenAiSettings) -> Dialect {
    if settings.reasoning_passback {
        let mut dialect = DEEPSEEK;
        // Rig's DeepSeek dialect names no request-id header; DeepSeek sends `x-request-id`.
        dialect.request_id_header = Some("x-request-id");
        dialect
    } else if settings.base_url.contains("api.openai.com") {
        OPENAI
    } else {
        let mut dialect = Dialect::gateway("openai_compat", "", "OPENAI_COMPAT_API_KEY");
        dialect.request_id_header = Some("x-request-id");
        dialect
    }
}

impl Provider for RigProvider {
    fn id(&self) -> &str {
        match self.backend {
            Backend::Anthropic(_) => "anthropic",
            Backend::OpenAiCompat(_) => "openai_compat",
        }
    }

    fn stream<'a>(&'a self, request: &'a ChatRequest) -> EventStream<'a> {
        match self.open(request, self.http.0.clone()) {
            Ok((streamed, names)) => {
                drive(streamed, names, self.backend.wire(), request.model.clone())
            }
            Err(message) => Box::pin(futures_util::stream::iter([ProviderEvent::Error {
                error: ProviderError {
                    code: "invalid_request".to_owned(),
                    message,
                    retryable: false,
                    request_id: None,
                },
            }])),
        }
    }
}

/// Captures the request instead of sending it.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Option<Preview>>>);

impl Capture {
    fn take(&self) -> Option<Preview> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl HttpClientExt for Capture {
    fn send<T, U>(
        &self,
        _req: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, HttpError>> + WasmCompatSend + 'static
    where
        T: Into<bytes::Bytes>,
        T: WasmCompatSend,
        U: From<bytes::Bytes>,
        U: WasmCompatSend + 'static,
    {
        std::future::ready(Err(HttpError::StreamEnded))
    }

    fn send_multipart<U>(
        &self,
        _req: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, HttpError>> + WasmCompatSend + 'static
    where
        U: From<bytes::Bytes>,
        U: WasmCompatSend + 'static,
    {
        std::future::ready(Err(HttpError::StreamEnded))
    }

    fn send_streaming<T>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse, HttpError>> + WasmCompatSend
    where
        T: Into<bytes::Bytes> + WasmCompatSend,
    {
        let (parts, body) = req.map(Into::into).into_parts();
        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| {
                let shown = if matches!(name.as_str(), "x-api-key" | "authorization" | "api-key") {
                    "***".to_owned()
                } else {
                    value.to_str().unwrap_or("<binary>").to_owned()
                };
                (name.as_str().to_owned(), shown)
            })
            .collect();
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(Preview {
            url: parts.uri.to_string(),
            headers,
            body: body.to_vec(),
        });
        std::future::ready(Err(HttpError::StreamEnded))
    }
}
