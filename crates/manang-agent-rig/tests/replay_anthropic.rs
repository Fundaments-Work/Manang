//! Spike check 1: request bodies for the recorded Anthropic thinking + tool_use conversation,
//! in-house `request_body()` against Rig's encoder (through the adapter).

mod common;

use common::fixtures::*;
use common::{Reply, chat, collect, diff, serve};
use manang_agent_core::*;
use serde_json::{Value, json};

/// The recorded conversation: user turn, the assistant turn the model produced (thinking with
/// signature, text, tool_use), the tool result, plus a redacted_thinking block.
async fn conversation(with_redacted: bool) -> ChatRequest {
    let server = serve(vec![Reply::sse(A_TOOL)]).await;
    let (inhouse, _) = anthropic_pair(&server);
    let mut request = chat("claude-sonnet-5-5", "draw a circle");
    request.tools = vec![tool_spec()];
    let events = collect(&inhouse, &request).await;
    let mut blocks: Vec<ContentBlock> = events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::Block { block } => Some(block.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(blocks.len(), 3);
    if with_redacted {
        let redacted = ContentBlock::from_json_str(
            r#"{"type":"redacted_thinking","data":"EmwKAhgBEgxS0tN0=="}"#,
        )
        .unwrap();
        blocks.insert(1, redacted);
    }
    request.messages.push(Message::new(Role::Assistant, blocks));
    request.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::tool_result("toolu_01", "ok", false).unwrap()],
    ));
    request
}

async fn bodies(request: &ChatRequest) -> (Vec<u8>, Vec<u8>) {
    let server = serve(vec![Reply::sse(A_TEXT)]).await;
    let (inhouse, rig) = anthropic_pair(&server);
    let a = inhouse.request_body(request).expect("in-house body");
    let b = rig.preview(request).await.expect("rig body").body;
    (a, b)
}

#[tokio::test]
async fn anthropic_thinking_tool_use_replay_report() {
    let request = conversation(false).await;
    let (a, b) = bodies(&request).await;
    let d = diff::report("anthropic thinking + tool_use", &a, &b);
    // Thinking signature and tool_use block are byte-exact inside Rig's body.
    let body = String::from_utf8(b).unwrap();
    for block in &request.messages[1].content {
        assert!(
            body.contains(block.raw_json()),
            "{} replays byte-exact",
            block.raw_json()
        );
    }
    let _ = d;
}

#[tokio::test]
async fn anthropic_redacted_thinking_replay_report() {
    let request = conversation(true).await;
    let (a, b) = bodies(&request).await;
    diff::report("anthropic with redacted_thinking", &a, &b);
    let body = String::from_utf8(b).unwrap();
    assert!(body.contains(r#"{"type":"redacted_thinking","data":"EmwKAhgBEgxS0tN0=="}"#));
    let _: Value = json!(null);
}

#[tokio::test]
async fn anthropic_cache_breakpoints_and_headers_report() {
    let server = serve(vec![Reply::sse(A_TEXT)]).await;
    let (inhouse, rig) = anthropic_pair(&server);
    let mut request = conversation(false).await;
    request.cache_breakpoints = vec![
        CacheBreakpoint::Tools,
        CacheBreakpoint::System,
        CacheBreakpoint::Message(2),
    ];
    let a = inhouse.request_body(&request).unwrap();
    let preview = rig.preview(&request).await.unwrap();
    diff::report("anthropic cache breakpoints", &a, &preview.body);
    println!("rig url: {}", preview.url);
    println!("rig headers: {:?}", preview.headers);
}
