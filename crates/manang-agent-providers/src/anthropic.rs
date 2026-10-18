//! Anthropic Messages API adapter (streaming).
//!
//! Rules this adapter keeps, from the API documentation (2026-10-18):
//! - no assistant prefill (400 on the 4.6+ family): a request whose last
//!   message is not from the user is refused locally;
//! - `tool_choice` is never sent (forced tool use is a 400 on 5.5/Fable 5.1);
//! - `thinking` and sampling parameters are never sent (adaptive thinking is
//!   the default, sampling overrides are rejected);
//! - history is replayed byte-exact: blocks are embedded as raw JSON, so
//!   thinking signatures survive, and nothing is reordered or edited. The
//!   only thing added to a block is `cache_control` where a breakpoint
//!   was requested, and only in the outgoing body, never in stored history.

use crate::driver::{Call, EventParser, error_event, failed, run};
use crate::key::ApiKey;
use crate::retry::RetryPolicy;
use crate::transport::{HttpRequest, HttpTransport};
use bytes::Bytes;
use manang_agent_core::{
    BlockKind, CacheBreakpoint, ChatRequest, ContentBlock, CoreError, EventStream, Provider,
    ProviderEvent, Role, SseEvent, StopReason, ToolNameMap, Usage,
};
use serde::Serialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// The default model.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// Model ids this release knows about (verified against the docs 2026-10-18).
pub const KNOWN_MODELS: &[&str] = &[
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-opus-5-5",
    "claude-fable-5-1",
];
/// The `anthropic-version` header value.
pub const API_VERSION: &str = "2023-06-01";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Connection settings. Contains the key: `Debug` hides it.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub base_url: String,
    pub api_key: ApiKey,
    pub version: String,
    pub retry: RetryPolicy,
    /// Allow only loopback base URLs (checked by resolving the host).
    pub offline: bool,
}

impl AnthropicConfig {
    pub fn new(api_key: ApiKey) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_key,
            version: API_VERSION.to_owned(),
            retry: RetryPolicy::default(),
            offline: false,
        }
    }
}

/// The Anthropic provider.
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    config: AnthropicConfig,
    transport: Arc<dyn HttpTransport>,
    cancel: CancellationToken,
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig, transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            config,
            transport,
            cancel: CancellationToken::new(),
        }
    }

    /// A copy whose requests stop when `cancel` fires.
    pub fn with_cancel(&self, cancel: CancellationToken) -> Self {
        Self {
            cancel,
            ..self.clone()
        }
    }

    /// The exact JSON body that `stream` would send. Contains no key; used by
    /// "Preview what will be sent" and by the audit log.
    pub fn request_body(&self, request: &ChatRequest) -> Result<Vec<u8>, CoreError> {
        build_body(request).map(|(body, _)| body)
    }

    fn url(&self) -> String {
        format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'))
    }
}

enum WireBlock<'a> {
    Borrowed(&'a RawValue),
    Owned(Box<RawValue>),
}

impl Serialize for WireBlock<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Borrowed(raw) => raw.serialize(s),
            Self::Owned(raw) => raw.serialize(s),
        }
    }
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: Role,
    content: Vec<WireBlock<'a>>,
}

#[derive(Serialize)]
struct Body<'a> {
    model: &'a str,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Value>,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
}

/// A thinking block without a signature came from another provider.
fn replayable(block: &ContentBlock) -> bool {
    block.kind() != BlockKind::Thinking || block.raw_json().contains("\"signature\"")
}

fn ephemeral() -> Value {
    json!({"type": "ephemeral"})
}

/// Adds `cache_control` to a copy of a raw block.
fn with_cache_control(block: &ContentBlock) -> Result<Box<RawValue>, CoreError> {
    let mut value: Value = serde_json::from_str(block.raw_json())?;
    if let Value::Object(map) = &mut value {
        map.insert("cache_control".to_owned(), ephemeral());
    }
    Ok(serde_json::value::to_raw_value(&value)?)
}

/// Builds the request body and the tool-name map for the response.
fn build_body(request: &ChatRequest) -> Result<(Vec<u8>, ToolNameMap), CoreError> {
    request.validate()?;
    if request.messages.last().map(|m| m.role) != Some(Role::User) {
        return Err(CoreError::InvalidRequest(
            "the conversation must end with a user message (assistant prefill is not supported)"
                .to_owned(),
        ));
    }
    let names = ToolNameMap::new(request.tools.iter().map(|t| t.name.as_str()))?;

    let breakpoint_on = |wanted: CacheBreakpoint| request.cache_breakpoints.contains(&wanted);

    let system = request.system.as_ref().map(|text| {
        if breakpoint_on(CacheBreakpoint::System) {
            json!([{"type": "text", "text": text, "cache_control": ephemeral()}])
        } else {
            Value::String(text.clone())
        }
    });

    let mut tools = Vec::with_capacity(request.tools.len());
    for tool in &request.tools {
        let wire = names.wire(&tool.name).unwrap_or(&tool.name);
        tools.push(json!({
            "name": wire,
            "description": tool.description,
            "input_schema": tool.input_schema,
        }));
    }
    if breakpoint_on(CacheBreakpoint::Tools)
        && let Some(Value::Object(last)) = tools.last_mut()
    {
        last.insert("cache_control".to_owned(), ephemeral());
    }

    let mut messages = Vec::with_capacity(request.messages.len());
    for (index, message) in request.messages.iter().enumerate() {
        let mark = breakpoint_on(CacheBreakpoint::Message(index));
        // Reasoning from other providers (no signature) cannot be replayed to
        // Anthropic: it would be rejected, so it is left out of the wire body.
        let blocks: Vec<&ContentBlock> = message.content.iter().filter(|b| replayable(b)).collect();
        if blocks.is_empty() {
            continue;
        }
        let marked = mark
            .then(|| {
                blocks.iter().rposition(|b| {
                    !matches!(b.kind(), BlockKind::Thinking | BlockKind::RedactedThinking)
                })
            })
            .flatten();
        let mut content = Vec::with_capacity(blocks.len());
        for (i, block) in blocks.iter().enumerate() {
            content.push(if marked == Some(i) {
                WireBlock::Owned(with_cache_control(block)?)
            } else {
                WireBlock::Borrowed(block.raw())
            });
        }
        messages.push(WireMessage {
            role: message.role,
            content,
        });
    }

    let body = Body {
        model: &request.model,
        max_tokens: request.max_tokens,
        stream: true,
        system,
        messages,
        tools,
    };
    Ok((serde_json::to_vec(&body)?, names))
}

impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn stream<'a>(&'a self, request: &'a ChatRequest) -> EventStream<'a> {
        let (body, names) = match build_body(request) {
            Ok(built) => built,
            Err(error) => return failed("invalid_request", error.to_string()),
        };
        let call = Call {
            transport: Arc::clone(&self.transport),
            request: HttpRequest {
                url: self.url(),
                headers: vec![
                    (
                        "x-api-key".to_owned(),
                        self.config.api_key.expose().to_owned(),
                    ),
                    ("anthropic-version".to_owned(), self.config.version.clone()),
                    ("content-type".to_owned(), "application/json".to_owned()),
                    ("accept".to_owned(), "text/event-stream".to_owned()),
                ],
                body: Bytes::from(body),
            },
            retry: self.config.retry,
            cancel: self.cancel.clone(),
            offline: self.config.offline,
            request_id_headers: &[],
        };
        run(call, AnthropicParser::new(names))
    }
}

struct Pending {
    block: Value,
    json: String,
}

/// Decodes the Anthropic event stream.
pub(crate) struct AnthropicParser {
    names: ToolNameMap,
    request_id: Option<String>,
    blocks: BTreeMap<u64, Pending>,
    usage: Usage,
    stop: Option<StopReason>,
}

impl AnthropicParser {
    pub(crate) fn new(names: ToolNameMap) -> Self {
        Self {
            names,
            request_id: None,
            blocks: BTreeMap::new(),
            usage: Usage::default(),
            stop: None,
        }
    }

    fn error(&self, code: &str, message: &str, retryable: bool) -> Vec<ProviderEvent> {
        vec![error_event(
            code,
            message,
            retryable,
            self.request_id.clone(),
        )]
    }

    fn absorb_usage(&mut self, usage: &Value) {
        let read = |key: &str| usage.get(key).and_then(Value::as_u64);
        if let Some(v) = read("input_tokens") {
            self.usage.input = v;
        }
        if let Some(v) = read("output_tokens") {
            self.usage.output = v;
        }
        if let Some(v) = read("cache_read_input_tokens") {
            self.usage.cache_read = v;
        }
        if let Some(v) = read("cache_creation_input_tokens") {
            self.usage.cache_write = v;
        }
    }

    fn delta(&mut self, index: u64, delta: &Value) -> Vec<ProviderEvent> {
        let Some(pending) = self.blocks.get_mut(&index) else {
            return Vec::new();
        };
        let append = |block: &mut Value, key: &str, extra: &str| {
            if let Some(Value::String(s)) = block.get_mut(key) {
                s.push_str(extra);
            } else if let Value::Object(map) = block {
                map.insert(key.to_owned(), Value::String(extra.to_owned()));
            }
        };
        let text_of = |key: &str| delta.get(key).and_then(Value::as_str).unwrap_or_default();
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                let text = text_of("text");
                append(&mut pending.block, "text", text);
                vec![ProviderEvent::TextDelta {
                    text: text.to_owned(),
                }]
            }
            Some("thinking_delta") => {
                let text = text_of("thinking");
                append(&mut pending.block, "thinking", text);
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![ProviderEvent::ThinkingDelta {
                        text: text.to_owned(),
                    }]
                }
            }
            Some("signature_delta") => {
                append(&mut pending.block, "signature", text_of("signature"));
                Vec::new()
            }
            Some("input_json_delta") => {
                pending.json.push_str(text_of("partial_json"));
                Vec::new()
            }
            Some("citations_delta") => {
                if let (Some(citation), Value::Object(map)) =
                    (delta.get("citation"), &mut pending.block)
                {
                    let list = map
                        .entry("citations".to_owned())
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(items) = list {
                        items.push(citation.clone());
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn block_stop(&mut self, index: u64) -> Vec<ProviderEvent> {
        let Some(Pending { mut block, json }) = self.blocks.remove(&index) else {
            return Vec::new();
        };
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let takes_input = matches!(
            kind.as_str(),
            "tool_use" | "server_tool_use" | "mcp_tool_use"
        );
        if takes_input && !json.trim().is_empty() {
            match serde_json::from_str::<Value>(&json) {
                Ok(input) => {
                    if let Value::Object(map) = &mut block {
                        map.insert("input".to_owned(), input);
                    }
                }
                Err(_) => {
                    return self.error(
                        "invalid_tool_input",
                        "the model produced tool input that is not valid JSON",
                        false,
                    );
                }
            }
        }
        let content = match ContentBlock::from_value(&block) {
            Ok(content) => content,
            Err(error) => return self.error("bad_block", &error.to_string(), false),
        };
        let mut out = vec![ProviderEvent::Block {
            block: content.clone(),
        }];
        if let Some(tool) = content.as_tool_use() {
            let name = self
                .names
                .internal(&tool.name)
                .unwrap_or(&tool.name)
                .to_owned();
            out.push(ProviderEvent::ToolUse {
                id: tool.id,
                name,
                input: tool.input,
            });
        }
        out
    }
}

fn stop_reason(raw: &str) -> StopReason {
    match raw {
        "end_turn" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "tool_use" => StopReason::ToolUse,
        "pause_turn" => StopReason::PauseTurn,
        "refusal" => StopReason::Refusal,
        other => StopReason::Other(other.to_owned()),
    }
}

fn error_retryable(kind: &str) -> bool {
    matches!(kind, "overloaded_error" | "api_error" | "timeout_error")
}

impl EventParser for AnthropicParser {
    fn set_request_id(&mut self, id: Option<String>) {
        self.request_id = id;
    }

    fn on_event(&mut self, event: &SseEvent) -> Vec<ProviderEvent> {
        let data: Value = match serde_json::from_str(&event.data) {
            Ok(v) => v,
            Err(_) => return self.error("bad_stream_json", "an event was not valid JSON", false),
        };
        let kind = data
            .get("type")
            .and_then(Value::as_str)
            .or(event.event.as_deref())
            .unwrap_or_default();
        let index = data.get("index").and_then(Value::as_u64).unwrap_or(0);
        match kind {
            "message_start" => {
                let message = data.get("message");
                if let Some(usage) = message.and_then(|m| m.get("usage")) {
                    self.absorb_usage(usage);
                }
                vec![ProviderEvent::Started {
                    request_id: self.request_id.clone(),
                    model: message
                        .and_then(|m| m.get("model"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }]
            }
            "content_block_start" => {
                let block = data.get("content_block").cloned().unwrap_or(Value::Null);
                let initial = block
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                    .map(|t| ProviderEvent::TextDelta { text: t.to_owned() });
                self.blocks.insert(
                    index,
                    Pending {
                        block,
                        json: String::new(),
                    },
                );
                initial.into_iter().collect()
            }
            "content_block_delta" => match data.get("delta") {
                Some(delta) => self.delta(index, delta),
                None => Vec::new(),
            },
            "content_block_stop" => self.block_stop(index),
            "message_delta" => {
                if let Some(raw) = data
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(Value::as_str)
                {
                    self.stop = Some(stop_reason(raw));
                }
                if let Some(usage) = data.get("usage") {
                    self.absorb_usage(usage);
                }
                Vec::new()
            }
            "message_stop" => vec![
                ProviderEvent::Usage { usage: self.usage },
                ProviderEvent::Done {
                    stop_reason: self
                        .stop
                        .take()
                        .unwrap_or_else(|| StopReason::Other("missing".to_owned())),
                },
            ],
            // An error after a 200 is terminal for this attempt: the partial
            // output already delivered cannot be unsent, so we never retry it.
            "error" => {
                let error = data.get("error");
                let code = error
                    .and_then(|e| e.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("stream_error");
                let message = error
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("the provider reported an error");
                let request_id = data
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| self.request_id.clone());
                vec![error_event(
                    code,
                    message,
                    error_retryable(code),
                    request_id,
                )]
            }
            // `ping` and event types added later are ignored.
            _ => Vec::new(),
        }
    }

    fn on_end(&mut self) -> Vec<ProviderEvent> {
        Vec::new()
    }
}
