mod common;

use common::{MockServer, Reply, chat, collect, serve};
use futures_util::StreamExt;
use manang_agent_core::*;
use manang_agent_providers::{
    AnthropicConfig, AnthropicProvider, ApiKey, ReqwestTransport, RetryPolicy, TransportConfig,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const TEXT: &str = include_str!("cassettes/anthropic_text.sse");
const TOOL: &str = include_str!("cassettes/anthropic_tool.sse");
const MID_ERROR: &str = include_str!("cassettes/anthropic_midstream_error.sse");

fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_secs(5),
    }
}

fn provider(server: &MockServer) -> AnthropicProvider {
    let mut config = AnthropicConfig::new(ApiKey::new("sk-test-123"));
    config.base_url = server.url();
    config.retry = fast_retry();
    let transport = ReqwestTransport::new(&TransportConfig::default()).expect("transport");
    AnthropicProvider::new(config, Arc::new(transport))
}

fn error_of(events: &[ProviderEvent]) -> &ProviderError {
    match events.last() {
        Some(ProviderEvent::Error { error }) => error,
        other => panic!("expected a terminal error, got {other:?}"),
    }
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
async fn text_stream_split_across_chunks_with_request_id_and_usage() {
    let reply = Reply::sse_split(TEXT, 7).header("request-id", "req_hdr_1");
    let server = serve(vec![reply]).await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;

    assert_eq!(
        events[0],
        ProviderEvent::Started {
            request_id: Some("req_hdr_1".into()),
            model: Some("claude-sonnet-5-5".into())
        }
    );
    let deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, ["Hello", " wörld"]);
    let block = events
        .iter()
        .find_map(|e| match e {
            ProviderEvent::Block { block } => Some(block),
            _ => None,
        })
        .expect("block");
    assert_eq!(block.as_text().as_deref(), Some("Hello wörld"));
    assert!(events.contains(&ProviderEvent::Usage {
        usage: Usage {
            input: 25,
            output: 15,
            cache_read: 10,
            cache_write: 3
        }
    }));
    assert_eq!(
        events.last(),
        Some(&ProviderEvent::Done {
            stop_reason: StopReason::EndTurn
        })
    );

    // What went over the wire.
    let sent = server.captured(0).await;
    assert_eq!(sent.path, "/v1/messages");
    assert_eq!(sent.header("x-api-key"), Some("sk-test-123"));
    assert_eq!(sent.header("anthropic-version"), Some("2023-06-01"));
    let body: Value = serde_json::from_str(&sent.body).unwrap();
    assert_eq!(body["stream"], true);
    assert_eq!(body["system"], "You are a careful scientific editor.");
    for forbidden in ["tool_choice", "temperature", "top_p", "thinking"] {
        assert!(
            body.get(forbidden).is_none(),
            "{forbidden} must not be sent"
        );
    }
}

#[tokio::test]
async fn tool_use_is_assembled_and_thinking_replays_byte_exact() {
    let server = serve(vec![Reply::sse_split(TOOL, 11)]).await;
    let p = provider(&server);
    let mut request = chat("claude-sonnet-5-5", "draw a circle");
    request.tools = vec![tool_spec()];
    let events = collect(&p, &request).await;

    let thinking_deltas: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, ProviderEvent::ThinkingDelta { .. }))
        .collect();
    assert_eq!(thinking_deltas.len(), 1);
    let blocks: Vec<ContentBlock> = events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::Block { block } => Some(block.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(blocks.len(), 3);
    assert_eq!(
        blocks[0].raw_json(),
        r#"{"type":"thinking","thinking":"Need a circle.","signature":"EqQBCgIYAhIM1gbc=="}"#
    );
    assert!(events.contains(&ProviderEvent::ToolUse {
        id: "toolu_01".into(),
        name: "figure.add_shape".into(),
        input: json!({"kind": "circle", "r": 4.5}),
    }));
    assert_eq!(
        events.last(),
        Some(&ProviderEvent::Done {
            stop_reason: StopReason::ToolUse
        })
    );
    let sent: Value = serde_json::from_str(&server.captured(0).await.body).unwrap();
    assert_eq!(sent["tools"][0]["name"], "figure__add_shape");

    // Replay: the next request carries the assistant blocks untouched.
    let mut next = request.clone();
    next.messages
        .push(Message::new(Role::Assistant, blocks.clone()));
    next.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::tool_result("toolu_01", "ok", false).unwrap()],
    ));
    let body = String::from_utf8(p.request_body(&next).unwrap()).unwrap();
    assert!(
        body.contains(blocks[0].raw_json()),
        "thinking block is byte-exact"
    );
    assert!(body.contains(blocks[2].raw_json()));
}

#[tokio::test]
async fn cache_breakpoints_land_on_tools_system_and_message() {
    let server = serve(vec![Reply::sse(TEXT)]).await;
    let p = provider(&server);
    let mut request = chat("claude-sonnet-5-5", "hi");
    request.tools = vec![tool_spec()];
    request.cache_breakpoints = vec![
        CacheBreakpoint::Tools,
        CacheBreakpoint::System,
        CacheBreakpoint::Message(0),
    ];
    let body: Value = serde_json::from_slice(&p.request_body(&request).unwrap()).unwrap();
    insta::assert_json_snapshot!(body);
    assert_eq!(
        String::from_utf8(p.request_body(&request).unwrap())
            .unwrap()
            .matches("cache_control")
            .count(),
        3
    );
}

#[tokio::test]
async fn prefill_is_refused_locally() {
    let server = serve(vec![Reply::sse(TEXT)]).await;
    let mut request = chat("claude-sonnet-5-5", "hi");
    request.messages.push(Message::new(
        Role::Assistant,
        vec![ContentBlock::text("Sure,").unwrap()],
    ));
    let events = collect(&provider(&server), &request).await;
    assert_eq!(error_of(&events).code, "invalid_request");
    assert_eq!(server.request_count().await, 0);
}

#[tokio::test]
async fn too_many_breakpoints_is_refused() {
    let server = serve(vec![Reply::sse(TEXT)]).await;
    let mut request = chat("claude-sonnet-5-5", "hi");
    request.cache_breakpoints = vec![CacheBreakpoint::System; 5];
    let events = collect(&provider(&server), &request).await;
    assert_eq!(error_of(&events).code, "invalid_request");
}

#[tokio::test]
async fn error_event_after_200_is_terminal_and_not_retried() {
    let server = serve(vec![Reply::sse(MID_ERROR)]).await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    assert!(events.contains(&ProviderEvent::TextDelta {
        text: "Partial".into()
    }));
    let error = error_of(&events);
    assert_eq!(error.code, "overloaded_error");
    assert!(error.retryable);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProviderEvent::Done { .. }))
    );
    assert_eq!(
        server.request_count().await,
        1,
        "no retry after output began"
    );
}

#[tokio::test]
async fn truncated_stream_is_an_error() {
    let cut = TEXT.split("event: message_delta").next().unwrap();
    let server = serve(vec![Reply::sse(cut)]).await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    let error = error_of(&events);
    assert_eq!(error.code, "stream_truncated");
    assert!(error.retryable);
}

#[tokio::test]
async fn invalid_tool_json_is_an_error() {
    let body = TOOL.replace("\\\"r\\\": 4", "\\\"r\\\": ,4");
    let server = serve(vec![Reply::sse(&body)]).await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    assert_eq!(error_of(&events).code, "invalid_tool_input");
}

const RATE_LIMIT: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"},"request_id":"req_body"}"#;

#[tokio::test]
async fn rate_limit_with_retry_after_then_success() {
    let server = serve(vec![
        Reply::status(429, RATE_LIMIT).header("retry-after", "0"),
        Reply::sse(TEXT),
    ])
    .await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));
    assert_eq!(server.request_count().await, 2);
}

#[tokio::test]
async fn rate_limit_without_retry_after_is_not_retried() {
    let server = serve(vec![
        Reply::status(429, RATE_LIMIT).header("request-id", "req_hdr"),
    ])
    .await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    let error = error_of(&events);
    assert_eq!(error.code, "rate_limit_error");
    assert_eq!(error.request_id.as_deref(), Some("req_hdr"));
    assert_eq!(server.request_count().await, 1);
}

#[tokio::test]
async fn auth_billing_and_bad_request_are_never_retried() {
    for (status, kind, hint) in [
        (401, "authentication_error", "check the API key"),
        (402, "billing_error", "add funds"),
        (400, "invalid_request_error", ""),
    ] {
        let body = format!(r#"{{"type":"error","error":{{"type":"{kind}","message":"nope"}}}}"#);
        // Even with a retry-after header.
        let server = serve(vec![
            Reply::status(status, &body).header("retry-after", "0"),
        ])
        .await;
        let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
        let error = error_of(&events);
        assert_eq!(error.code, kind);
        assert!(!error.retryable);
        assert!(error.message.contains(hint), "{}", error.message);
        assert_eq!(server.request_count().await, 1, "status {status}");
    }
}

#[tokio::test]
async fn overloaded_is_retried_up_to_the_limit() {
    let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
    let server = serve(vec![Reply::status(529, body)]).await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    let error = error_of(&events);
    assert_eq!(error.code, "overloaded_error");
    assert!(error.retryable);
    assert_eq!(server.request_count().await, 3, "max_attempts");
}

#[tokio::test]
async fn server_error_then_success() {
    let server = serve(vec![
        Reply::status(
            500,
            r#"{"type":"error","error":{"type":"api_error","message":"boom"}}"#,
        ),
        Reply::sse(TEXT),
    ])
    .await;
    let events = collect(&provider(&server), &chat("claude-sonnet-5-5", "hi")).await;
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));
}

fn partial_stream() -> String {
    TEXT.split("event: content_block_stop")
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn dropping_the_stream_closes_the_connection() {
    let server = serve(vec![Reply::sse(&partial_stream()).held()]).await;
    let p = provider(&server);
    let request = chat("claude-sonnet-5-5", "hi");
    let mut stream = p.stream(&request);
    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
            Ok(Some(ProviderEvent::TextDelta { .. })) => break,
            Ok(Some(_)) => {}
            other => panic!("stream ended early: {other:?}"),
        }
    }
    drop(stream);
    tokio::time::timeout(Duration::from_secs(5), server.client_closed.notified())
        .await
        .expect("the server saw the connection close");
}

#[tokio::test]
async fn cancel_token_ends_with_a_cancelled_event() {
    let server = serve(vec![Reply::sse(&partial_stream()).held()]).await;
    let token = CancellationToken::new();
    let p = provider(&server).with_cancel(token.clone());
    let request = chat("claude-sonnet-5-5", "hi");
    let mut stream = p.stream(&request);
    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
            Ok(Some(ProviderEvent::TextDelta { .. })) => break,
            Ok(Some(_)) => {}
            other => panic!("stream ended early: {other:?}"),
        }
    }
    token.cancel();
    // Events already buffered may still arrive; the cancelled error follows.
    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
            Ok(Some(ProviderEvent::Error { error })) => {
                assert_eq!(error.code, "cancelled");
                break;
            }
            Ok(Some(ProviderEvent::TextDelta { .. })) => {}
            other => panic!("expected cancelled, got {other:?}"),
        }
    }
    assert!(stream.next().await.is_none());
    drop(stream);
    tokio::time::timeout(Duration::from_secs(5), server.client_closed.notified())
        .await
        .expect("connection closed after cancel");
}

#[test]
fn the_key_never_appears_in_debug_output() {
    let config = AnthropicConfig::new(ApiKey::new("sk-test-123"));
    assert!(!format!("{config:?}").contains("sk-test-123"));
    let request = manang_agent_providers::HttpRequest {
        url: "http://x".into(),
        headers: vec![("x-api-key".into(), "sk-test-123".into())],
        body: bytes::Bytes::new(),
    };
    let shown = format!("{request:?}");
    assert!(!shown.contains("sk-test-123") && shown.contains("x-api-key"));
}
