mod common;

use common::{MockServer, Reply, chat, collect, serve};
use manang_agent_core::*;
use manang_agent_providers::{
    ApiKey, OpenAiCompatConfig, OpenAiCompatProvider, ReqwestTransport, RetryPolicy,
    TransportConfig,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const OLLAMA: &str = include_str!("cassettes/ollama_text.sse");
const DS_TEXT: &str = include_str!("cassettes/deepseek_text.sse");
const DS_REASONING: &str = include_str!("cassettes/deepseek_reasoning.sse");
const DS_TOOLS: &str = include_str!("cassettes/deepseek_tools.sse");
const DS_402: &str = include_str!("cassettes/deepseek_402.json");
const DS_429: &str = include_str!("cassettes/deepseek_429.json");

fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_secs(5),
    }
}

fn provider_with(config: OpenAiCompatConfig, server: &MockServer) -> OpenAiCompatProvider {
    let mut config = config;
    config.base_url = format!("{}/v1", server.url());
    config.retry = fast_retry();
    let transport = ReqwestTransport::new(&TransportConfig::default()).expect("transport");
    OpenAiCompatProvider::new(config, Arc::new(transport))
}

fn ollama(server: &MockServer) -> OpenAiCompatProvider {
    provider_with(OpenAiCompatConfig::new("", None), server)
}

fn deepseek(server: &MockServer) -> OpenAiCompatProvider {
    let preset = preset_by_id("deepseek").unwrap();
    let config =
        OpenAiCompatConfig::from_preset(&preset, None, Some(ApiKey::new("ds-key-1"))).unwrap();
    assert_eq!(config.base_url, "https://api.deepseek.com");
    provider_with(config, server)
}

fn error_of(events: &[ProviderEvent]) -> &ProviderError {
    match events.last() {
        Some(ProviderEvent::Error { error }) => error,
        other => panic!("expected a terminal error, got {other:?}"),
    }
}

fn blocks_of(events: &[ProviderEvent]) -> Vec<ContentBlock> {
    events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::Block { block } => Some(block.clone()),
            _ => None,
        })
        .collect()
}

fn tool_spec() -> ToolSpec {
    ToolSpec {
        name: "figure.add_shape".into(),
        title: None,
        description: "Adds a shape".into(),
        input_schema: json!({"type": "object", "properties": {"kind": {"type": "string"}}}),
        output_schema: None,
        risk: RiskClass::Propose,
    }
}

#[tokio::test]
async fn ollama_style_stream_without_a_key() {
    let server = serve(vec![Reply::sse_split(OLLAMA, 5)]).await;
    let events = collect(&ollama(&server), &chat("llama3.2", "hi")).await;
    assert_eq!(
        events[0],
        ProviderEvent::Started {
            request_id: None,
            model: Some("llama3.2".into())
        }
    );
    assert!(events.contains(&ProviderEvent::TextDelta { text: "Hi".into() }));
    assert_eq!(blocks_of(&events)[0].as_text().as_deref(), Some("Hi there"));
    assert!(events.contains(&ProviderEvent::Usage {
        usage: Usage {
            input: 9,
            output: 3,
            cache_read: 0,
            cache_write: 0
        }
    }));
    assert_eq!(
        events.last(),
        Some(&ProviderEvent::Done {
            stop_reason: StopReason::EndTurn
        })
    );
    let sent = server.captured(0).await;
    assert_eq!(sent.path, "/v1/chat/completions");
    assert!(sent.header("authorization").is_none());
    let body: Value = serde_json::from_str(&sent.body).unwrap();
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert_eq!(body["max_tokens"], 1024);
    assert_eq!(body["messages"][0]["role"], "system");
    assert!(body.get("tool_choice").is_none());
}

#[tokio::test]
async fn bearer_and_custom_key_headers() {
    let server = serve(vec![Reply::sse(OLLAMA)]).await;
    let bearer = provider_with(
        OpenAiCompatConfig::new("", Some(ApiKey::new("k-1"))),
        &server,
    );
    collect(&bearer, &chat("m", "hi")).await;
    assert_eq!(
        server.captured(0).await.header("authorization"),
        Some("Bearer k-1")
    );

    let mut config = OpenAiCompatConfig::new("", Some(ApiKey::new("k-2")));
    config.key_header = Some("api-key".into());
    collect(&provider_with(config, &server), &chat("m", "hi")).await;
    let sent = server.captured(1).await;
    assert_eq!(sent.header("api-key"), Some("k-2"));
    assert!(sent.header("authorization").is_none());
}

#[tokio::test]
async fn max_completion_tokens_and_extra_body() {
    let server = serve(vec![Reply::sse(OLLAMA)]).await;
    let mut config = OpenAiCompatConfig::new("", None);
    config.max_tokens_field = MaxTokensField::MaxCompletionTokens;
    config
        .extra_body
        .insert("reasoning_effort".into(), json!("low"));
    config.extra_body.insert("stream".into(), json!(false)); // must not override
    let p = provider_with(config, &server);
    let body: Value = serde_json::from_slice(&p.request_body(&chat("m", "hi")).unwrap()).unwrap();
    assert_eq!(body["max_completion_tokens"], 1024);
    assert!(body.get("max_tokens").is_none());
    assert_eq!(body["reasoning_effort"], "low");
    assert_eq!(body["stream"], true);
}

#[tokio::test]
async fn deepseek_text_stream_maps_cache_hit_and_miss_usage() {
    let server = serve(vec![
        Reply::sse_split(DS_TEXT, 9).header("x-request-id", "ds_req_1"),
    ])
    .await;
    let events = collect(&deepseek(&server), &chat("deepseek-flash", "hello")).await;
    assert_eq!(
        events[0],
        ProviderEvent::Started {
            request_id: Some("ds_req_1".into()),
            model: Some("deepseek-flash".into())
        }
    );
    assert_eq!(
        blocks_of(&events)[0].as_text().as_deref(),
        Some("Hello there")
    );
    assert!(events.contains(&ProviderEvent::Usage {
        usage: Usage {
            input: 40,
            output: 20,
            cache_read: 60,
            cache_write: 0
        }
    }));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProviderEvent::ThinkingDelta { .. }))
    );
    assert_eq!(
        server.captured(0).await.header("authorization"),
        Some("Bearer ds-key-1")
    );
    // The cost estimate distinguishes hits from misses.
    let cost = PriceTable::builtin()
        .estimate(
            "deepseek-flash",
            &Usage {
                input: 40,
                output: 20,
                cache_read: 60,
                cache_write: 0,
            },
        )
        .unwrap();
    assert!(cost.estimate);
}

#[tokio::test]
async fn deepseek_reasoning_stream() {
    let server = serve(vec![Reply::sse_split(DS_REASONING, 13)]).await;
    let events = collect(&deepseek(&server), &chat("deepseek-flash", "hi")).await;
    let thinking: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::ThinkingDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thinking, ["The user says ", "hi."]);
    let blocks = blocks_of(&events);
    assert_eq!(blocks[0].kind(), BlockKind::Thinking);
    assert_eq!(
        blocks[0].raw_json(),
        r#"{"type":"thinking","thinking":"The user says hi."}"#
    );
    assert_eq!(blocks[1].as_text().as_deref(), Some("Hi!"));
    assert!(events.contains(&ProviderEvent::TextDelta { text: "Hi!".into() }));
}

#[tokio::test]
async fn deepseek_tool_call_assembly_and_reasoning_passback() {
    let server = serve(vec![Reply::sse_split(DS_TOOLS, 17)]).await;
    let p = deepseek(&server);
    let mut request = chat("deepseek-flash", "draw a circle");
    request.tools = vec![tool_spec()];
    let events = collect(&p, &request).await;
    assert!(events.contains(&ProviderEvent::ToolUse {
        id: "call_0_abc".into(),
        name: "figure.add_shape".into(),
        input: json!({"kind": "circle"}),
    }));
    assert_eq!(
        events.last(),
        Some(&ProviderEvent::Done {
            stop_reason: StopReason::ToolUse
        })
    );
    let sent: Value = serde_json::from_str(&server.captured(0).await.body).unwrap();
    assert_eq!(sent["tools"][0]["type"], "function");
    assert_eq!(sent["tools"][0]["function"]["name"], "figure__add_shape");

    // Next request: DeepSeek needs the reasoning back when tools are used.
    let blocks = blocks_of(&events);
    let mut next = request.clone();
    next.messages
        .push(Message::new(Role::Assistant, blocks.clone()));
    next.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::tool_result("call_0_abc", "added", false).unwrap()],
    ));
    let body: Value = serde_json::from_slice(&p.request_body(&next).unwrap()).unwrap();
    insta::assert_json_snapshot!("deepseek_second_request", body);
    let assistant = &body["messages"][2];
    assert_eq!(assistant["reasoning_content"], "I should call the tool.");
    assert_eq!(assistant["tool_calls"][0]["id"], "call_0_abc");
    assert_eq!(body["messages"][3]["role"], "tool");

    // A server without passback (Ollama) never sees reasoning_content, and
    // Anthropic-signed thinking blocks are dropped too.
    let plain = ollama(&server);
    let body: Value = serde_json::from_slice(&plain.request_body(&next).unwrap()).unwrap();
    assert!(body["messages"][2].get("reasoning_content").is_none());
}

#[tokio::test]
async fn deepseek_402_is_actionable_and_never_retried() {
    let server = serve(vec![Reply::status(402, DS_402).header("retry-after", "0")]).await;
    let events = collect(&deepseek(&server), &chat("deepseek-flash", "hi")).await;
    let error = error_of(&events);
    assert_eq!(error.code, "insufficient_balance");
    assert!(!error.retryable);
    assert!(error.message.contains("Insufficient Balance") && error.message.contains("add funds"));
    assert_eq!(server.request_count().await, 1);
}

#[tokio::test]
async fn deepseek_401_and_422_are_never_retried() {
    for (status, code) in [(401, "authentication_error"), (422, "invalid_parameters")] {
        let body = r#"{"error":{"message":"bad","type":"unknown_error","code":"x"}}"#;
        let server = serve(vec![Reply::status(status, body)]).await;
        let events = collect(&deepseek(&server), &chat("deepseek-flash", "hi")).await;
        assert_eq!(error_of(&events).code, code);
        assert_eq!(server.request_count().await, 1);
    }
}

#[tokio::test]
async fn deepseek_429_without_retry_after_is_not_retried_and_503_is() {
    let server = serve(vec![Reply::status(429, DS_429)]).await;
    let events = collect(&deepseek(&server), &chat("deepseek-flash", "hi")).await;
    assert_eq!(error_of(&events).code, "rate_limit_error");
    assert_eq!(server.request_count().await, 1);

    let server = serve(vec![
        Reply::status(503, r#"{"error":{"message":"busy","type":"server_error"}}"#),
        Reply::sse(DS_TEXT),
    ])
    .await;
    let events = collect(&deepseek(&server), &chat("deepseek-flash", "hi")).await;
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));
    assert_eq!(server.request_count().await, 2);
}

#[tokio::test]
async fn error_object_inside_the_stream_is_terminal() {
    let body = "data: {\"error\":{\"message\":\"boom\",\"type\":\"server_error\"}}\n\n";
    let server = serve(vec![Reply::sse(body)]).await;
    let events = collect(&ollama(&server), &chat("m", "hi")).await;
    assert_eq!(error_of(&events).code, "server_error");
}

#[tokio::test]
async fn finish_reason_without_done_marker_still_completes() {
    let body =
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"length\"}]}\n\n";
    let server = serve(vec![Reply::sse(body)]).await;
    let events = collect(&ollama(&server), &chat("m", "hi")).await;
    assert_eq!(
        events.last(),
        Some(&ProviderEvent::Done {
            stop_reason: StopReason::MaxTokens
        })
    );
}

#[tokio::test]
async fn missing_finish_and_done_is_truncation() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n";
    let server = serve(vec![Reply::sse(body)]).await;
    let events = collect(&ollama(&server), &chat("m", "hi")).await;
    assert_eq!(error_of(&events).code, "stream_truncated");
}
