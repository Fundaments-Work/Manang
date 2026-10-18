//! Spike check 2: request bodies for the recorded DeepSeek `reasoning_content` + tool-call
//! conversation, in-house `request_body()` against Rig's encoder.

mod common;

use common::fixtures::*;
use common::{Reply, chat, collect, diff, serve};
use manang_agent_core::*;
use serde_json::Value;

async fn conversation() -> ChatRequest {
    let server = serve(vec![Reply::sse(DS_TOOLS)]).await;
    let (inhouse, _) = deepseek_pair(&server);
    let mut request = chat("deepseek-flash", "draw a circle");
    request.tools = vec![tool_spec()];
    let events = collect(&inhouse, &request).await;
    let blocks: Vec<ContentBlock> = events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::Block { block } => Some(block.clone()),
            _ => None,
        })
        .collect();
    request.messages.push(Message::new(Role::Assistant, blocks));
    request.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::tool_result("call_0_abc", "added", false).unwrap()],
    ));
    request
}

#[tokio::test]
async fn deepseek_reasoning_content_passback_report() {
    let request = conversation().await;
    let server = serve(vec![Reply::sse(DS_TEXT)]).await;
    let (inhouse, rig) = deepseek_pair(&server);
    let a = inhouse.request_body(&request).unwrap();
    let preview = rig.preview(&request).await.unwrap();
    println!("rig url: {}", preview.url);
    println!("rig headers: {:?}", preview.headers);
    diff::report("deepseek reasoning_content + tool call", &a, &preview.body);
    let b: Value = serde_json::from_slice(&preview.body).unwrap();
    let assistant = &b["messages"][2];
    assert_eq!(assistant["reasoning_content"], "I should call the tool.");
    assert_eq!(assistant["tool_calls"][0]["id"], "call_0_abc");
    assert_eq!(b["messages"][3]["role"], "tool");
}

#[tokio::test]
async fn ollama_style_without_passback_report() {
    let request = conversation().await;
    let server = serve(vec![Reply::sse(OLLAMA)]).await;
    let (inhouse, rig) = ollama_pair(&server);
    let a = inhouse.request_body(&request).unwrap();
    let preview = rig.preview(&request).await.unwrap();
    println!("rig headers: {:?}", preview.headers);
    diff::report("ollama-style, no reasoning passback", &a, &preview.body);
    let b: Value = serde_json::from_slice(&preview.body).unwrap();
    assert!(b["messages"][2].get("reasoning_content").is_none());
}
