//! Spike checks 3, 4, 6: streaming parity with the in-house adapters over the recorded
//! cassettes, cancellation by drop, and error mapping.

mod common;

use common::fixtures::*;
use common::{MockServer, Reply, chat, collect, serve};
use futures_util::StreamExt;
use manang_agent_core::*;
use std::time::Duration;

const RATE_LIMIT: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"},"request_id":"req_body"}"#;

/// Runs the same scripted server for both providers; returns (in-house, rig) events and the
/// number of requests each saw.
async fn both(
    script: Vec<Reply>,
    req: &ChatRequest,
    kind: &str,
) -> (Vec<ProviderEvent>, Vec<ProviderEvent>, usize, usize) {
    let s1 = serve(script.clone()).await;
    let s2 = serve(script).await;
    let (a, b) = pair(kind, &s1, &s2);
    let ea = collect(a.as_ref(), req).await;
    let eb = collect(b.as_ref(), req).await;
    (ea, eb, s1.request_count().await, s2.request_count().await)
}

fn pair(kind: &str, s1: &MockServer, s2: &MockServer) -> (Box<dyn Provider>, Box<dyn Provider>) {
    match kind {
        "anthropic" => (
            Box::new(anthropic_pair(s1).0),
            Box::new(anthropic_pair(s2).1),
        ),
        "deepseek" => (Box::new(deepseek_pair(s1).0), Box::new(deepseek_pair(s2).1)),
        _ => (Box::new(ollama_pair(s1).0), Box::new(ollama_pair(s2).1)),
    }
}

fn show(name: &str, a: &[ProviderEvent], b: &[ProviderEvent]) {
    println!("==== {name}: equal = {}", a == b);
    if a != b {
        for e in a {
            println!("  in-house {e:?}");
        }
        for e in b {
            println!("  rig      {e:?}");
        }
    }
}

fn tools_req(model: &str) -> ChatRequest {
    let mut r = chat(model, "draw a circle");
    r.tools = vec![tool_spec()];
    r
}

#[tokio::test]
async fn anthropic_text() {
    let r = chat("claude-sonnet-5-5", "hi");
    let (a, b, na, nb) = both(
        vec![Reply::sse_split(A_TEXT, 7).header("request-id", "req_hdr_1")],
        &r,
        "anthropic",
    )
    .await;
    show("anthropic text", &a, &b);
    assert_eq!((na, nb), (1, 1));
    assert_eq!(a, b);
}

#[tokio::test]
async fn anthropic_tool_with_thinking() {
    let (a, b, ..) = both(
        vec![Reply::sse_split(A_TOOL, 11)],
        &tools_req("claude-sonnet-5-5"),
        "anthropic",
    )
    .await;
    show("anthropic tool+thinking", &a, &b);
    assert_eq!(a, b);
}

#[tokio::test]
async fn anthropic_midstream_error() {
    let r = chat("claude-sonnet-5-5", "hi");
    let (a, b, na, nb) = both(vec![Reply::sse(A_MID_ERROR)], &r, "anthropic").await;
    show("anthropic mid-stream error", &a, &b);
    assert_eq!((na, nb), (1, 1), "no retry after output began");
    assert_eq!(a, b);
}

#[tokio::test]
async fn anthropic_truncated() {
    let cut = A_TEXT.split("event: message_delta").next().unwrap();
    let r = chat("claude-sonnet-5-5", "hi");
    let (a, b, ..) = both(vec![Reply::sse(cut)], &r, "anthropic").await;
    show("anthropic truncated", &a, &b);
    assert_eq!(a, b);
}

#[tokio::test]
async fn anthropic_429_then_success() {
    let r = chat("claude-sonnet-5-5", "hi");
    let (a, b, na, nb) = both(
        vec![
            Reply::status(429, RATE_LIMIT).header("retry-after", "0"),
            Reply::sse(A_TEXT),
        ],
        &r,
        "anthropic",
    )
    .await;
    show("anthropic 429 then success", &a, &b);
    assert_eq!((na, nb), (2, 2));
    assert!(matches!(b.last(), Some(ProviderEvent::Done { .. })));
    assert_eq!(a, b);
}

#[tokio::test]
async fn anthropic_429_without_retry_after_and_auth_errors() {
    let r = chat("claude-sonnet-5-5", "hi");
    let (a, b, na, nb) = both(
        vec![Reply::status(429, RATE_LIMIT).header("request-id", "req_hdr")],
        &r,
        "anthropic",
    )
    .await;
    show("anthropic 429 no retry-after", &a, &b);
    assert_eq!((na, nb), (1, 1));
    assert_eq!(a, b);
    for (status, kind) in [
        (401, "authentication_error"),
        (402, "billing_error"),
        (400, "invalid_request_error"),
    ] {
        let body = format!(r#"{{"type":"error","error":{{"type":"{kind}","message":"nope"}}}}"#);
        let (a, b, na, nb) = both(
            vec![Reply::status(status, &body).header("retry-after", "0")],
            &r,
            "anthropic",
        )
        .await;
        show(&format!("anthropic {status}"), &a, &b);
        assert_eq!((na, nb), (1, 1), "{status} not retried");
        assert_eq!(a, b);
    }
}

#[tokio::test]
async fn anthropic_overloaded_is_retried_up_to_the_limit() {
    let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
    let r = chat("claude-sonnet-5-5", "hi");
    let (a, b, na, nb) = both(vec![Reply::status(529, body)], &r, "anthropic").await;
    show("anthropic 529", &a, &b);
    assert_eq!((na, nb), (3, 3));
    assert_eq!(a, b);
}

#[tokio::test]
async fn deepseek_text_reasoning_tools() {
    for (name, cassette, req) in [
        ("deepseek text", DS_TEXT, chat("deepseek-flash", "hello")),
        (
            "deepseek reasoning",
            DS_REASONING,
            chat("deepseek-flash", "hi"),
        ),
        ("deepseek tools", DS_TOOLS, tools_req("deepseek-flash")),
    ] {
        let (a, b, ..) = both(
            vec![Reply::sse_split(cassette, 13).header("x-request-id", "ds_req_1")],
            &req,
            "deepseek",
        )
        .await;
        show(name, &a, &b);
        assert_eq!(a, b, "{name}");
    }
}

#[tokio::test]
async fn ollama_text() {
    let r = chat("llama3.2", "hi");
    let (a, b, ..) = both(vec![Reply::sse_split(OLLAMA, 5)], &r, "ollama").await;
    show("ollama text", &a, &b);
    assert_eq!(a, b);
}

#[tokio::test]
async fn deepseek_errors() {
    let r = chat("deepseek-flash", "hi");
    for (name, script, expect_requests) in [
        (
            "402",
            vec![Reply::status(402, DS_402).header("retry-after", "0")],
            1,
        ),
        ("429 no retry-after", vec![Reply::status(429, DS_429)], 1),
        (
            "401",
            vec![Reply::status(
                401,
                r#"{"error":{"message":"bad","type":"unknown_error","code":"x"}}"#,
            )],
            1,
        ),
        (
            "422",
            vec![Reply::status(
                422,
                r#"{"error":{"message":"bad","type":"unknown_error","code":"x"}}"#,
            )],
            1,
        ),
        (
            "503 then ok",
            vec![
                Reply::status(503, r#"{"error":{"message":"busy","type":"server_error"}}"#),
                Reply::sse(DS_TEXT),
            ],
            2,
        ),
    ] {
        let (a, b, na, nb) = both(script, &r, "deepseek").await;
        show(&format!("deepseek {name}"), &a, &b);
        assert_eq!((na, nb), (expect_requests, expect_requests), "{name}");
        assert_eq!(a, b, "{name}");
    }
}

#[tokio::test]
async fn deepseek_error_object_in_stream_and_missing_done() {
    let r = chat("m", "hi");
    for (name, body) in [
        (
            "error object",
            "data: {\"error\":{\"message\":\"boom\",\"type\":\"server_error\"}}\n\n",
        ),
        (
            "finish without DONE",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"length\"}]}\n\n",
        ),
        (
            "no finish no DONE",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
        ),
    ] {
        let (a, b, ..) = both(vec![Reply::sse(body)], &r, "ollama").await;
        show(name, &a, &b);
    }
}

#[tokio::test]
async fn dropping_the_rig_stream_closes_the_connection() {
    let partial = A_TEXT.split("event: content_block_stop").next().unwrap();
    let server = serve(vec![Reply::sse(partial).held()]).await;
    let (_, rig) = anthropic_pair(&server);
    let request = chat("claude-sonnet-5-5", "hi");
    let mut stream = rig.stream(&request);
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
