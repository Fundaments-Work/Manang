//! `ChatRequest` (our types) to a Rig `CompletionRequest`.
//!
//! The mapping keeps what the in-house adapters guarantee:
//! - history is replayed losslessly: every Anthropic block we hold is attached to its canonical
//!   Rig block as the `native` provider item, so signatures and `redacted_thinking` payloads go
//!   back byte for byte (Rig replays the native item while it is current and the turn's origin
//!   matches the target model);
//! - reasoning without a signature (from another provider) is not replayed to Anthropic;
//! - tool names go through our `ToolNameMap` (`figure.add_shape` becomes `figure__add_shape`);
//! - `tool_choice`, sampling parameters and `thinking` are never set by us.

use manang_agent_core::{BlockKind, ChatRequest, ContentBlock, Role, ToolNameMap};
use rig_core::completion::{CompletionRequest, ToolDefinition};
use rig_core::message::{
    AssistantContent, AssistantMessage, CallId, Message, Origin, Reasoning, Text, ToolName,
    ToolResult, ToolResultContent, UserContent,
};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Which wire the request is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wire {
    /// Anthropic Messages: provider items replay verbatim.
    Anthropic,
    /// OpenAI-compatible chat: replay from canonical blocks.
    OpenAiCompat,
}

/// A request Rig can send, and the map to restore our tool names.
pub(crate) struct Encoded {
    pub request: CompletionRequest,
    pub names: ToolNameMap,
    /// Ask Rig for its manual cache markers (system, last tool, last message block).
    pub prompt_caching: bool,
}

/// Why a request could not be built.
#[derive(Debug, thiserror::Error)]
pub(crate) enum EncodeError {
    #[error("{0}")]
    Invalid(String),
}

impl From<manang_agent_core::CoreError> for EncodeError {
    fn from(error: manang_agent_core::CoreError) -> Self {
        Self::Invalid(error.to_string())
    }
}

fn tool_name(name: &str) -> Result<ToolName, EncodeError> {
    ToolName::new(name).map_err(|_| EncodeError::Invalid("a tool has an empty name".to_owned()))
}

fn block_value(block: &ContentBlock) -> Result<Value, EncodeError> {
    serde_json::from_str(block.raw_json())
        .map_err(|e| EncodeError::Invalid(format!("content block is not JSON: {e}")))
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Text of a `tool_result` block's `content` (a string or a list of text parts).
fn tool_result_text(value: &Value) -> String {
    match value.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The assistant block as a Rig block. With `native`, the raw provider item rides along.
fn assistant_block(
    block: &ContentBlock,
    wire: Wire,
    reasoning_field: Option<&str>,
    names_in_history: &mut HashMap<String, String>,
) -> Result<Option<AssistantContent>, EncodeError> {
    let value = block_value(block)?;
    let native = wire == Wire::Anthropic;
    let content = match block.kind() {
        BlockKind::Text => AssistantContent::Text(Text::new(str_of(&value, "text"))),
        BlockKind::Thinking => {
            // A thinking block without a signature came from another provider. Anthropic would
            // reject it; DeepSeek wants its text back.
            if wire == Wire::Anthropic && value.get("signature").is_none() {
                return Ok(None);
            }
            let reasoning = AssistantContent::Reasoning(Reasoning::new(str_of(&value, "thinking")));
            // OpenAI-style wires keep reasoning only as a native `{field: text}` item.
            return Ok(match (wire, reasoning_field) {
                (Wire::OpenAiCompat, Some(field)) => {
                    let item = serde_json::json!({ field: str_of(&value, "thinking") });
                    Some(reasoning.with_native(item))
                }
                (Wire::OpenAiCompat, None) => None,
                (Wire::Anthropic, _) => Some(reasoning.with_native(value)),
            });
        }
        BlockKind::RedactedThinking => {
            if wire != Wire::Anthropic {
                return Ok(None);
            }
            AssistantContent::Reasoning(Reasoning {
                text: String::new(),
                redacted: true,
                native: None,
            })
        }
        BlockKind::ToolUse => {
            let id = str_of(&value, "id");
            let name = str_of(&value, "name");
            names_in_history.insert(id.to_owned(), name.to_owned());
            AssistantContent::tool_call(
                id,
                tool_name(name)?,
                value.get("input").cloned().unwrap_or(Value::Null),
            )
        }
        BlockKind::ToolResult => return Ok(None),
        BlockKind::Other => {
            return Ok(if native {
                Some(AssistantContent::Opaque(rig_core::message::Opaque {
                    item: value,
                    replay: true,
                }))
            } else {
                None
            });
        }
    };
    Ok(Some(if native {
        content.with_native(value)
    } else {
        content
    }))
}

fn user_content(
    block: &ContentBlock,
    names_in_history: &HashMap<String, String>,
) -> Result<Option<UserContent>, EncodeError> {
    let value = block_value(block)?;
    Ok(match block.kind() {
        BlockKind::Text => Some(UserContent::text(str_of(&value, "text"))),
        BlockKind::ToolResult => {
            let id = str_of(&value, "tool_use_id");
            let name = names_in_history
                .get(id)
                .map(String::as_str)
                .unwrap_or("tool");
            let is_error = value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(UserContent::ToolResult(ToolResult {
                call: CallId::from_wire(id),
                name: tool_name(name)?,
                content: vec![ToolResultContent::text(tool_result_text(&value))],
                is_error,
            }))
        }
        // Images, documents and other raw user blocks: not forwarded by this adapter.
        _ => None,
    })
}

/// Builds the Rig request. `anthropic_origin` names the target for provider-item replay.
pub(crate) fn encode(
    request: &ChatRequest,
    wire: Wire,
    origin: &Origin,
    reasoning_field: Option<&str>,
    extra_body: &Map<String, Value>,
) -> Result<Encoded, EncodeError> {
    request.validate()?;
    if wire == Wire::Anthropic && request.messages.last().map(|m| m.role) != Some(Role::User) {
        return Err(EncodeError::Invalid(
            "the conversation must end with a user message (assistant prefill is not supported)"
                .to_owned(),
        ));
    }
    let names = ToolNameMap::new(request.tools.iter().map(|t| t.name.as_str()))?;

    let mut history: Vec<Message> = Vec::new();
    if let Some(system) = &request.system {
        history.push(Message::system(system.as_str()));
    }
    let mut calls: HashMap<String, String> = HashMap::new();
    for message in &request.messages {
        match message.role {
            Role::User => {
                let mut content = Vec::new();
                for block in &message.content {
                    content.extend(user_content(block, &calls)?);
                }
                if !content.is_empty() {
                    history.push(Message::User { content });
                }
            }
            Role::Assistant => {
                let mut content = Vec::new();
                for block in &message.content {
                    content.extend(assistant_block(block, wire, reasoning_field, &mut calls)?);
                }
                if content.is_empty() {
                    continue;
                }
                let turn = AssistantMessage::new(content).with_origin(origin.clone());
                history.push(Message::Assistant(turn));
            }
        }
    }

    let mut tools = Vec::with_capacity(request.tools.len());
    for tool in &request.tools {
        let wire_name = names.wire(&tool.name).unwrap_or(&tool.name);
        tools.push(ToolDefinition::new(
            tool_name(wire_name)?,
            tool.description.clone(),
            tool.input_schema.clone(),
        ));
    }

    let mut rig = CompletionRequest::new(Message::user(""))
        .model(request.model.clone())
        .max_tokens(u64::from(request.max_tokens))
        .tools(tools);
    rig.chat_history = history;

    // Cache breakpoints: Rig places Anthropic markers by rule (system, last tool, last block of
    // the last message), not at arbitrary message indexes. Any breakpoint asks for the rule.
    let prompt_caching = wire == Wire::Anthropic && !request.cache_breakpoints.is_empty();

    if !extra_body.is_empty() {
        rig.additional_params = Some(Value::Object(extra_body.clone()));
    }
    Ok(Encoded {
        request: rig,
        names,
        prompt_caching,
    })
}
