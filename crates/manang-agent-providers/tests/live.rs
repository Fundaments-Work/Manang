//! Live tests against real providers. Ignored by default, and they only run
//! with `MANANG_EVAL_LIVE=1` plus the provider's key in its environment
//! variable. They cost a few cents at most.
//!
//! DeepSeek (the owner runs this):
//!
//! ```text
//! DEEPSEEK_API_KEY=... MANANG_EVAL_LIVE=1 cargo test -p manang-agent-providers --locked -- --ignored deepseek --nocapture
//! ```
//!
//! Anthropic:
//!
//! ```text
//! ANTHROPIC_API_KEY=... MANANG_EVAL_LIVE=1 cargo test -p manang-agent-providers --locked -- --ignored anthropic --nocapture
//! ```

use futures_util::StreamExt;
use manang_agent_core::*;
use manang_agent_providers::{
    AnthropicConfig, AnthropicProvider, ApiKey, OpenAiCompatConfig, OpenAiCompatProvider,
    ReqwestTransport, TransportConfig,
};
use serde_json::json;
use std::sync::Arc;

fn key_for(var: &str) -> Option<ApiKey> {
    if std::env::var("MANANG_EVAL_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skipped: set MANANG_EVAL_LIVE=1 to run live tests");
        return None;
    }
    match std::env::var(var) {
        Ok(key) if !key.is_empty() => Some(ApiKey::new(&key)),
        _ => {
            eprintln!("skipped: {var} is not set");
            None
        }
    }
}

fn transport() -> Arc<ReqwestTransport> {
    Arc::new(ReqwestTransport::new(&TransportConfig::default()).expect("transport"))
}

fn demo_tool() -> ToolSpec {
    ToolSpec {
        name: "demo.add".into(),
        title: None,
        description: "Adds two integers and returns the sum.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}},
            "required": ["a", "b"],
        }),
        output_schema: None,
        risk: RiskClass::Read,
    }
}

fn request(model: &str, prompt: &str, tools: Vec<ToolSpec>) -> ChatRequest {
    ChatRequest {
        model: model.to_owned(),
        system: Some("Be brief.".to_owned()),
        messages: vec![Message::user_text(prompt).expect("message")],
        tools,
        max_tokens: 2048,
        cache_breakpoints: vec![],
    }
}

async fn run(provider: &dyn Provider, request: &ChatRequest) -> Vec<ProviderEvent> {
    let events: Vec<ProviderEvent> = provider.stream(request).collect().await;
    for event in &events {
        if let ProviderEvent::Error { error } = event {
            panic!(
                "provider error: {} ({}): {}",
                error.code, error.retryable, error.message
            );
        }
    }
    events
}

fn text_of(events: &[ProviderEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

async fn hello_and_tool_round_trip(provider: &dyn Provider, model: &str) {
    // One-shot hello.
    let events = run(
        provider,
        &request(model, "Say hello in one short sentence.", vec![]),
    )
    .await;
    let text = text_of(&events);
    println!("hello -> {text}");
    assert!(!text.trim().is_empty());
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ProviderEvent::Usage { .. }))
    );

    // A tiny tool round trip.
    let tools = vec![demo_tool()];
    let mut req = request(
        model,
        "Use the demo.add tool to add 2 and 3, then tell me the sum.",
        tools,
    );
    let events = run(provider, &req).await;
    let (id, name, input) = events
        .iter()
        .find_map(|e| match e {
            ProviderEvent::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .expect("the model called the tool");
    assert_eq!(name, "demo.add");
    let sum = input["a"].as_i64().unwrap_or(0) + input["b"].as_i64().unwrap_or(0);
    req.messages.push(Message::new(
        Role::Assistant,
        events
            .iter()
            .filter_map(|e| match e {
                ProviderEvent::Block { block } => Some(block.clone()),
                _ => None,
            })
            .collect(),
    ));
    req.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::tool_result(&id, &sum.to_string(), false).expect("result")],
    ));
    let events = run(provider, &req).await;
    let text = text_of(&events);
    println!("after tool -> {text}");
    assert!(text.contains('5'), "the answer mentions the sum: {text}");
}

#[tokio::test]
#[ignore = "live: needs MANANG_EVAL_LIVE=1 and DEEPSEEK_API_KEY"]
async fn deepseek_live_hello_and_tool_round_trip() {
    let Some(key) = key_for("DEEPSEEK_API_KEY") else {
        return;
    };
    let preset = preset_by_id("deepseek").expect("preset");
    let config = OpenAiCompatConfig::from_preset(&preset, None, Some(key)).expect("config");
    let provider = OpenAiCompatProvider::new(config, transport());
    hello_and_tool_round_trip(&provider, &preset.models[0]).await;
}

#[tokio::test]
#[ignore = "live: needs MANANG_EVAL_LIVE=1 and ANTHROPIC_API_KEY"]
async fn anthropic_live_hello_and_tool_round_trip() {
    let Some(key) = key_for("ANTHROPIC_API_KEY") else {
        return;
    };
    let provider = AnthropicProvider::new(AnthropicConfig::new(key), transport());
    hello_and_tool_round_trip(&provider, "claude-haiku-5-5").await;
}
