//! Rig's stream items to our `ProviderEvent`s.

use crate::encode::Wire;
use crate::error::map_error;
use futures_util::StreamExt;
use manang_agent_core::{ContentBlock, EventStream, ProviderEvent, StopReason, ToolNameMap, Usage};
use rig_core::completion::{FinishReason, Usage as RigUsage};
use rig_core::message::AssistantContent;
use rig_core::streaming::{CompletionStream, Item, StreamEvent};
use serde_json::{Value, json};

/// The block we hand to history for a finished Rig block. Anthropic keeps the provider item
/// verbatim (signatures survive); other wires get the canonical shapes the in-house adapters
/// produce (`thinking` without a signature, `text`, `tool_use`).
fn block_of(content: &AssistantContent, wire: Wire) -> Option<ContentBlock> {
    if wire == Wire::Anthropic
        && let Some(item) = native_of(content)
    {
        return ContentBlock::from_value(item).ok();
    }
    let value = match content {
        AssistantContent::Text(text) => json!({"type": "text", "text": text.text}),
        AssistantContent::Reasoning(r) => json!({"type": "thinking", "thinking": r.text}),
        AssistantContent::ToolCall(call) => json!({
            "type": "tool_use",
            "id": call.id.wire(),
            "name": call.function.name.as_str(),
            "input": call.function.arguments,
        }),
        AssistantContent::Opaque(opaque) => opaque.item.clone(),
        AssistantContent::Image(_) => return None,
        _ => return None,
    };
    ContentBlock::from_value(&value).ok()
}

fn native_of(content: &AssistantContent) -> Option<&Value> {
    content.native_item().or(match content {
        AssistantContent::Opaque(opaque) => Some(&opaque.item),
        _ => None,
    })
}

fn usage_of(usage: &RigUsage, wire: Wire) -> Usage {
    let input = usage.input_tokens.unwrap_or(0);
    let cached = usage.cached_input_tokens.unwrap_or(0);
    Usage {
        // Anthropic reports input_tokens without the cached part; OpenAI-style APIs include it.
        input: match wire {
            // Rig folds the cache read and write counts into Anthropic's `input_tokens`.
            Wire::Anthropic => input
                .saturating_sub(cached)
                .saturating_sub(usage.cache_creation_input_tokens.unwrap_or(0)),
            Wire::OpenAiCompat => input.saturating_sub(cached),
        },
        output: usage.output_tokens.unwrap_or(0),
        cache_read: cached,
        cache_write: usage.cache_creation_input_tokens.unwrap_or(0),
    }
}

fn stop_reason(reason: Option<&FinishReason>) -> StopReason {
    match reason {
        Some(FinishReason::Stop) | None => StopReason::EndTurn,
        Some(FinishReason::Length) => StopReason::MaxTokens,
        Some(FinishReason::ToolCalls) => StopReason::ToolUse,
        Some(FinishReason::ContentFilter) => StopReason::Refusal,
        Some(FinishReason::Other(raw)) => match raw.as_str() {
            "stop_sequence" => StopReason::StopSequence,
            "pause_turn" => StopReason::PauseTurn,
            "refusal" => StopReason::Refusal,
            other => StopReason::Other(other.to_owned()),
        },
        Some(_) => StopReason::Other("unknown".to_owned()),
    }
}

/// The block, and for a tool call the parsed call, for a finished Rig part.
fn end_events(content: &AssistantContent, wire: Wire, names: &ToolNameMap) -> Vec<ProviderEvent> {
    let mut out = Vec::new();
    if let Some(block) = block_of(content, wire) {
        out.push(ProviderEvent::Block { block });
    }
    if let AssistantContent::ToolCall(call) = content {
        let wire_name = call.function.name.as_str();
        out.push(ProviderEvent::ToolUse {
            id: call.id.wire().into_owned(),
            name: names.internal(wire_name).unwrap_or(wire_name).to_owned(),
            input: Value::Object(call.function.arguments.clone()),
        });
    }
    out
}

fn error_event(error: manang_agent_core::ProviderError) -> ProviderEvent {
    ProviderEvent::Error { error }
}

/// Drives a Rig completion stream as our event stream. Dropping the returned stream drops the
/// Rig stream, which closes the connection.
pub(crate) fn drive(
    streamed: CompletionStream,
    names: ToolNameMap,
    wire: Wire,
    model: String,
) -> EventStream<'static> {
    Box::pin(async_stream::stream! {
        let mut streamed = streamed;
        let mut started = false;
        let mut held: Vec<(usize, AssistantContent)> = Vec::new();
        while let Some(item) = streamed.next().await {
            let event = match item {
                Ok(Item::Event(event)) => event,
                Ok(Item::Unknown(_)) => continue,
                Err(error) => {
                    yield error_event(map_error(&error, started));
                    return;
                }
            };
            if !started {
                started = true;
                let partial = streamed.partial();
                yield ProviderEvent::Started {
                    request_id: partial.provider_request_id.clone(),
                    // Rig fills the provider-reported model only when the reply ends; until then the
                    // requested model stands in.
                    model: partial.origin.response_model.clone().or(Some(model.clone())),
                };
            }
            match event {
                StreamEvent::Text { text, .. } if !text.is_empty() => {
                    yield ProviderEvent::TextDelta { text };
                }
                StreamEvent::Reasoning { text, .. } if !text.is_empty() => {
                    yield ProviderEvent::ThinkingDelta { text };
                }
                StreamEvent::End { part, content } => {
                    if wire == Wire::Anthropic {
                        for event in end_events(&content, wire, &names) {
                            yield event;
                        }
                    } else {
                        // Rig closes a reasoning part after the text that follows it; the
                        // in-house adapter emits blocks in message order at the end.
                        held.push((part.index(), content));
                    }
                }
                _ => {}
            }
        }
        held.sort_by_key(|(index, _)| *index);
        for (_, content) in &held {
            for event in end_events(content, wire, &names) {
                yield event;
            }
        }
        match streamed.finish().await {
            Ok(response) => {
                if response.usage.is_reported() {
                    yield ProviderEvent::Usage { usage: usage_of(&response.usage, wire) };
                }
                yield ProviderEvent::Done { stop_reason: stop_reason(response.finish_reason().as_ref()) };
            }
            Err(error) => yield error_event(map_error(&error, started)),
        }
    })
}
