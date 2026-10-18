//! OpenAI-compatible Chat Completions adapter (streaming, tools).
//!
//! Works against OpenAI-style servers: DeepSeek, OpenRouter, Groq, Together,
//! Ollama (`http://localhost:11434/v1`, any dummy key), llama.cpp's server,
//! LM Studio, vLLM and company gateways. The Responses API is out of scope.
//!
//! Protocol notes:
//! - Anthropic-style thinking blocks cannot be expressed here and are dropped
//!   from the outgoing history. Reasoning the server streams as
//!   `reasoning_content` (DeepSeek) or `reasoning` (OpenRouter, Ollama) is
//!   surfaced as `ThinkingDelta` and kept as a signature-less `thinking`
//!   block. With `reasoning_passback` (DeepSeek) it is sent back as
//!   `reasoning_content` on assistant messages, which DeepSeek requires when
//!   tools are used (HTTP 400 otherwise, per its Thinking Mode guide).
//! - `tool_choice` is never sent; thinking-mode DeepSeek rejects `required`.
//! - Cache breakpoints are ignored; DeepSeek caches prefixes automatically and
//!   reports `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`.

use crate::driver::{Call, EventParser, error_event, failed, run};
use crate::key::ApiKey;
use crate::retry::RetryPolicy;
use crate::transport::{HttpRequest, HttpTransport};
use bytes::Bytes;
use manang_agent_core::{
    BlockKind, ChatRequest, ContentBlock, CoreError, EventStream, MaxTokensField, Provider,
    ProviderEvent, ProviderKind, ProviderPreset, Role, SseEvent, StopReason, ToolNameMap, Usage,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// Where a stock Ollama listens.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";

/// Connection settings. `Debug` hides the key.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// Includes the version path, e.g. `http://localhost:11434/v1`.
    pub base_url: String,
    /// `None` sends no key header.
    pub api_key: Option<ApiKey>,
    /// Header that carries the key; `None` means `Authorization: Bearer <key>`.
    /// A custom name receives the bare key.
    pub key_header: Option<String>,
    pub max_tokens_field: MaxTokensField,
    /// Send the model's reasoning back on assistant messages (DeepSeek).
    pub reasoning_passback: bool,
    /// Extra top-level request fields (for example DeepSeek's `thinking` or
    /// `reasoning_effort`). Cannot override `model`, `messages` or `stream`.
    pub extra_body: Map<String, Value>,
    pub retry: RetryPolicy,
    /// Allow only loopback base URLs (checked by resolving the host).
    pub offline: bool,
}

impl OpenAiCompatConfig {
    pub fn new(base_url: &str, api_key: Option<ApiKey>) -> Self {
        Self {
            base_url: base_url.to_owned(),
            api_key,
            key_header: None,
            max_tokens_field: MaxTokensField::MaxTokens,
            reasoning_passback: false,
            extra_body: Map::new(),
            retry: RetryPolicy::default(),
            offline: false,
        }
    }

    /// Builds the config for a preset. `base_url` overrides the preset's URL
    /// (required for the custom preset). Fails for Anthropic presets.
    pub fn from_preset(
        preset: &ProviderPreset,
        base_url: Option<&str>,
        api_key: Option<ApiKey>,
    ) -> Result<Self, String> {
        if preset.kind != ProviderKind::OpenAiCompat {
            return Err(format!(
                "`{}` is not an OpenAI-compatible preset",
                preset.id
            ));
        }
        let url = base_url
            .or(preset.base_url.as_deref())
            .ok_or_else(|| "this preset needs a base URL".to_owned())?;
        let mut config = Self::new(url, api_key);
        config.key_header.clone_from(&preset.key_header);
        config.max_tokens_field = preset.max_tokens_field;
        config.reasoning_passback = preset.reasoning_passback;
        Ok(config)
    }
}

/// The OpenAI-compatible provider.
#[derive(Debug, Clone)]
pub struct OpenAiCompatProvider {
    config: OpenAiCompatConfig,
    transport: Arc<dyn HttpTransport>,
    cancel: CancellationToken,
}

impl OpenAiCompatProvider {
    pub fn new(config: OpenAiCompatConfig, transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            config,
            transport,
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_cancel(&self, cancel: CancellationToken) -> Self {
        Self {
            cancel,
            ..self.clone()
        }
    }

    /// The exact JSON body `stream` would send (no key in it).
    pub fn request_body(&self, request: &ChatRequest) -> Result<Vec<u8>, CoreError> {
        build_body(request, &self.config).map(|(body, _)| body)
    }

    fn url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }
}

/// Id and text of a `tool_result` block (`content` is a string or a list).
fn tool_result_text(block: &ContentBlock) -> (String, String) {
    let value: Value = serde_json::from_str(block.raw_json()).unwrap_or(Value::Null);
    let id = value
        .get("tool_use_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let text = match value.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    (id, text)
}

fn thinking_text(block: &ContentBlock) -> String {
    serde_json::from_str::<Value>(block.raw_json())
        .ok()
        .and_then(|v| v.get("thinking").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default()
}

fn build_body(
    request: &ChatRequest,
    config: &OpenAiCompatConfig,
) -> Result<(Vec<u8>, ToolNameMap), CoreError> {
    request.validate()?;
    let names = ToolNameMap::new(request.tools.iter().map(|t| t.name.as_str()))?;
    let mut messages: Vec<Value> = Vec::new();
    if let Some(system) = &request.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    for message in &request.messages {
        match message.role {
            Role::User => {
                let mut text = String::new();
                for block in &message.content {
                    match block.kind() {
                        BlockKind::ToolResult => {
                            let (id, content) = tool_result_text(block);
                            messages.push(
                                json!({"role": "tool", "tool_call_id": id, "content": content}),
                            );
                        }
                        BlockKind::Text => text.push_str(&block.as_text().unwrap_or_default()),
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    messages.push(json!({"role": "user", "content": text}));
                }
            }
            Role::Assistant => {
                let mut text = String::new();
                let mut reasoning = String::new();
                let mut calls = Vec::new();
                for block in &message.content {
                    if let Some(tool) = block.as_tool_use() {
                        calls.push(json!({
                            "id": tool.id,
                            "type": "function",
                            "function": {"name": tool.name, "arguments": tool.input.to_string()},
                        }));
                    } else if let Some(t) = block.as_text() {
                        text.push_str(&t);
                    } else if block.kind() == BlockKind::Thinking {
                        reasoning.push_str(&thinking_text(block));
                    }
                }
                if text.is_empty() && calls.is_empty() {
                    continue;
                }
                let mut entry = json!({"role": "assistant", "content": text});
                if config.reasoning_passback && !reasoning.is_empty() {
                    entry["reasoning_content"] = Value::String(reasoning);
                }
                if !calls.is_empty() {
                    entry["tool_calls"] = Value::Array(calls);
                }
                messages.push(entry);
            }
        }
    }
    let limit_field = match config.max_tokens_field {
        MaxTokensField::MaxTokens => "max_tokens",
        MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
    };
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
    });
    body[limit_field] = json!(request.max_tokens);
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                json!({"type": "function", "function": {
                    "name": names.wire(&t.name).unwrap_or(&t.name),
                    "description": t.description,
                    "parameters": t.input_schema,
                }})
            })
            .collect();
        body["tools"] = Value::Array(tools);
    }
    for (key, value) in &config.extra_body {
        if !matches!(key.as_str(), "model" | "messages" | "stream") {
            body[key.as_str()] = value.clone();
        }
    }
    Ok((serde_json::to_vec(&body)?, names))
}

impl Provider for OpenAiCompatProvider {
    fn id(&self) -> &str {
        "openai_compat"
    }

    fn stream<'a>(&'a self, request: &'a ChatRequest) -> EventStream<'a> {
        let (body, names) = match build_body(request, &self.config) {
            Ok(built) => built,
            Err(error) => return failed("invalid_request", error.to_string()),
        };
        let mut headers = vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("accept".to_owned(), "text/event-stream".to_owned()),
        ];
        if let Some(key) = &self.config.api_key {
            match &self.config.key_header {
                Some(name) => headers.push((name.clone(), key.expose().to_owned())),
                None => headers.push((
                    "authorization".to_owned(),
                    format!("Bearer {}", key.expose()),
                )),
            }
        }
        let call = Call {
            transport: Arc::clone(&self.transport),
            request: HttpRequest {
                url: self.url(),
                headers,
                body: Bytes::from(body),
            },
            retry: self.config.retry,
            cancel: self.cancel.clone(),
            offline: self.config.offline,
            request_id_headers: &["x-request-id"],
        };
        run(call, OpenAiParser::new(names))
    }
}

#[derive(Default)]
struct PendingCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

pub(crate) struct OpenAiParser {
    names: ToolNameMap,
    request_id: Option<String>,
    started: bool,
    finished: bool,
    text: String,
    reasoning: String,
    calls: BTreeMap<u64, PendingCall>,
    usage: Option<Usage>,
    finish: Option<StopReason>,
}

impl OpenAiParser {
    pub(crate) fn new(names: ToolNameMap) -> Self {
        Self {
            names,
            request_id: None,
            started: false,
            finished: false,
            text: String::new(),
            reasoning: String::new(),
            calls: BTreeMap::new(),
            usage: None,
            finish: None,
        }
    }

    /// Maps OpenAI usage, including DeepSeek's cache hit/miss counts.
    fn absorb_usage(&mut self, usage: &Value) {
        let read = |v: &Value, key: &str| v.get(key).and_then(Value::as_u64);
        let prompt = read(usage, "prompt_tokens").unwrap_or(0);
        let hit = read(usage, "prompt_cache_hit_tokens")
            .or_else(|| {
                usage
                    .get("prompt_tokens_details")
                    .and_then(|d| read(d, "cached_tokens"))
            })
            .unwrap_or(0);
        let miss =
            read(usage, "prompt_cache_miss_tokens").unwrap_or_else(|| prompt.saturating_sub(hit));
        self.usage = Some(Usage {
            input: miss,
            output: read(usage, "completion_tokens").unwrap_or(0),
            cache_read: hit,
            cache_write: 0,
        });
    }

    /// Emits the assembled blocks, usage and `Done`. Runs once.
    fn finalize(&mut self) -> Vec<ProviderEvent> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        let mut out = Vec::new();
        if !self.reasoning.is_empty()
            && let Ok(block) =
                ContentBlock::from_value(&json!({"type": "thinking", "thinking": self.reasoning}))
        {
            out.push(ProviderEvent::Block { block });
        }
        if !self.text.is_empty()
            && let Ok(block) = ContentBlock::text(&self.text)
        {
            out.push(ProviderEvent::Block { block });
        }
        let had_calls = !self.calls.is_empty();
        for (index, call) in std::mem::take(&mut self.calls) {
            let id = call.id.unwrap_or_else(|| format!("call_{index}"));
            let input = if call.arguments.trim().is_empty() {
                Value::Object(Map::new())
            } else {
                match serde_json::from_str::<Value>(&call.arguments) {
                    Ok(v) => v,
                    Err(_) => {
                        return vec![error_event(
                            "invalid_tool_input",
                            "the model produced tool arguments that are not valid JSON",
                            false,
                            self.request_id.clone(),
                        )];
                    }
                }
            };
            if let Ok(block) = ContentBlock::tool_use(&id, &call.name, &input) {
                out.push(ProviderEvent::Block { block });
            }
            let name = self
                .names
                .internal(&call.name)
                .unwrap_or(&call.name)
                .to_owned();
            out.push(ProviderEvent::ToolUse { id, name, input });
        }
        if let Some(usage) = self.usage {
            out.push(ProviderEvent::Usage { usage });
        }
        let stop_reason = match self.finish.take() {
            Some(StopReason::EndTurn) | None if had_calls => StopReason::ToolUse,
            Some(reason) => reason,
            None => StopReason::EndTurn,
        };
        out.push(ProviderEvent::Done { stop_reason });
        out
    }
}

fn finish_reason(raw: &str) -> StopReason {
    match raw {
        "stop" => StopReason::EndTurn,
        "length" => StopReason::MaxTokens,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "content_filter" => StopReason::Refusal,
        // Includes DeepSeek's `insufficient_system_resource` and `aborted`.
        other => StopReason::Other(other.to_owned()),
    }
}

impl EventParser for OpenAiParser {
    fn set_request_id(&mut self, id: Option<String>) {
        self.request_id = id;
    }

    fn on_event(&mut self, event: &SseEvent) -> Vec<ProviderEvent> {
        let data = event.data.trim();
        if data == "[DONE]" {
            return self.finalize();
        }
        let value: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => {
                return vec![error_event(
                    "bad_stream_json",
                    "an event was not valid JSON",
                    false,
                    self.request_id.clone(),
                )];
            }
        };
        if let Some(error) = value.get("error") {
            let text = |key: &str| error.get(key).and_then(Value::as_str);
            let code = text("type")
                .or_else(|| text("code"))
                .unwrap_or("stream_error");
            let message = text("message").unwrap_or("the provider reported an error");
            return vec![error_event(code, message, false, self.request_id.clone())];
        }
        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            out.push(ProviderEvent::Started {
                request_id: self.request_id.clone(),
                model: value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
        }
        if let Some(usage) = value.get("usage").filter(|u| u.is_object()) {
            self.absorb_usage(usage);
        }
        let Some(choice) = value.get("choices").and_then(|c| c.get(0)) else {
            return out;
        };
        if let Some(delta) = choice.get("delta") {
            let reasoning = delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty());
            if let Some(text) = reasoning {
                self.reasoning.push_str(text);
                out.push(ProviderEvent::ThinkingDelta {
                    text: text.to_owned(),
                });
            }
            if let Some(text) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                self.text.push_str(text);
                out.push(ProviderEvent::TextDelta {
                    text: text.to_owned(),
                });
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
                    let entry = self.calls.entry(index).or_default();
                    if let Some(id) = call
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                    {
                        entry.id = Some(id.to_owned());
                    }
                    if let Some(function) = call.get("function") {
                        if let Some(name) = function.get("name").and_then(Value::as_str) {
                            entry.name.push_str(name);
                        }
                        if let Some(args) = function.get("arguments").and_then(Value::as_str) {
                            entry.arguments.push_str(args);
                        }
                    }
                }
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish = Some(finish_reason(reason));
        }
        out
    }

    fn on_end(&mut self) -> Vec<ProviderEvent> {
        // Some servers omit `[DONE]`; a finish reason is proof of completion.
        if self.finish.is_some() {
            self.finalize()
        } else {
            Vec::new()
        }
    }
}
