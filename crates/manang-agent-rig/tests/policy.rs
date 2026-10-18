//! Spike check 5: policy plumbing. Our reqwest client (loopback-only DNS, no redirects), the
//! audit/redaction tap, our retry rules and request-id/usage reach Rig's call path.

mod common;

use common::fixtures::*;
use common::{Reply, chat, collect, serve};
use manang_agent_core::*;
use manang_agent_rig as rig;
use std::sync::{Arc, Mutex};

fn code_of(events: &[ProviderEvent]) -> String {
    match events.last() {
        Some(ProviderEvent::Error { error }) => error.code.clone(),
        other => panic!("expected error, got {other:?}"),
    }
}

fn ollama_provider(base: &str, policy: rig::HttpPolicy) -> rig::RigProvider {
    let settings = rig::OpenAiSettings::new(base, None);
    rig::RigProvider::new(rig::Backend::OpenAiCompat(settings), rig_http(policy))
}

fn offline() -> rig::HttpPolicy {
    rig::HttpPolicy {
        loopback_only: true,
        retry: fast_retry(),
        ..rig::HttpPolicy::default()
    }
}

#[tokio::test]
async fn loopback_works_and_non_loopback_is_blocked_offline() {
    let server = serve(vec![Reply::sse(OLLAMA)]).await;
    let p = ollama_provider(&format!("{}/v1", server.url()), offline());
    let events = collect(&p, &chat("m", "hi")).await;
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));

    // IP literals: refused before any connection.
    for base in ["http://192.0.2.1:9/v1", "http://[2001:db8::1]:9/v1"] {
        let p = ollama_provider(base, offline());
        assert_eq!(
            code_of(&collect(&p, &chat("m", "hi")).await),
            "offline_blocked",
            "{base}"
        );
    }
    // A hostname that resolves nowhere safe: the loopback-only resolver (not a string check)
    // refuses it at connect time. It surfaces as a connect failure, never a connection.
    let p = ollama_provider("https://api.openai.com.invalid/v1", offline());
    let code = code_of(&collect(&p, &chat("m", "hi")).await);
    assert!(
        code == "connect_failed" || code == "offline_blocked",
        "hostname blocked at connect time, got {code}"
    );
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let target = serve(vec![Reply::sse(OLLAMA)]).await;
    let origin =
        serve(vec![Reply::status(302, "").header(
            "location",
            &format!("{}/v1/chat/completions", target.url()),
        )])
        .await;
    let p = ollama_provider(&format!("{}/v1", origin.url()), rig_policy());
    let events = collect(&p, &chat("m", "hi")).await;
    assert_eq!(code_of(&events), "http_302");
    assert_eq!(target.request_count().await, 0);
}

type Seen = (String, Vec<(String, String)>, String);

struct Tap {
    seen: Mutex<Vec<Seen>>,
}

impl rig::RequestTap for Tap {
    fn on_request(&self, url: &str, headers: &[(String, String)], body: &[u8]) -> Option<Vec<u8>> {
        let text = String::from_utf8_lossy(body).into_owned();
        self.seen
            .lock()
            .unwrap()
            .push((url.to_owned(), headers.to_vec(), text.clone()));
        // Our redaction: here, a stand-in secret in the prompt.
        Some(text.replace("SECRET-PROJECT", "[redacted]").into_bytes())
    }
}

#[tokio::test]
async fn tap_sees_the_final_request_and_can_redact_before_send() {
    let server = serve(vec![Reply::sse(A_TEXT).header("request-id", "req_hdr_9")]).await;
    let tap = Arc::new(Tap {
        seen: Mutex::new(Vec::new()),
    });
    let policy = rig::HttpPolicy {
        tap: Some(tap.clone()),
        ..rig_policy()
    };
    let mut settings = rig::AnthropicSettings::new(rig::ApiKey::new("sk-test-123"));
    settings.base_url = server.url();
    let p = rig::RigProvider::new(rig::Backend::Anthropic(settings), rig_http(policy));
    let events = collect(&p, &chat("claude-sonnet-5-5", "about SECRET-PROJECT")).await;

    // (b) the tap saw the exact final body and headers, with the key masked.
    let seen = tap.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].2.contains("SECRET-PROJECT"));
    assert!(
        seen[0]
            .1
            .iter()
            .any(|(k, v)| k == "x-api-key" && v == "***")
    );
    assert!(!format!("{:?}", seen[0].1).contains("sk-test-123"));
    // ... and the server received the redacted body, with the real key.
    let sent = server.captured(0).await;
    assert!(!sent.body.contains("SECRET-PROJECT") && sent.body.contains("[redacted]"));
    assert_eq!(sent.header("x-api-key"), Some("sk-test-123"));
    // (d) request id and cache usage arrive.
    assert!(matches!(
        &events[0],
        ProviderEvent::Started { request_id: Some(id), .. } if id == "req_hdr_9"
    ));
    assert!(events.contains(&ProviderEvent::Usage {
        usage: Usage {
            input: 25,
            output: 15,
            cache_read: 10,
            cache_write: 3
        }
    }));
}

#[tokio::test]
async fn our_retry_rules_wrap_the_rig_call_path() {
    // (c) 429 without retry-after: not retried. 500 then success: retried once.
    let server = serve(vec![Reply::status(
        429,
        r#"{"type":"error","error":{"type":"rate_limit_error","message":"x"}}"#,
    )])
    .await;
    let (_, p) = anthropic_pair(&server);
    assert_eq!(
        code_of(&collect(&p, &chat("claude-sonnet-5-5", "hi")).await),
        "rate_limit_error"
    );
    assert_eq!(server.request_count().await, 1);

    let server = serve(vec![
        Reply::status(
            500,
            r#"{"type":"error","error":{"type":"api_error","message":"boom"}}"#,
        ),
        Reply::sse(A_TEXT),
    ])
    .await;
    let (_, p) = anthropic_pair(&server);
    let events = collect(&p, &chat("claude-sonnet-5-5", "hi")).await;
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));
    assert_eq!(server.request_count().await, 2);
}

#[test]
fn the_key_never_appears_in_debug_output() {
    let settings = rig::AnthropicSettings::new(rig::ApiKey::new("sk-test-123"));
    assert!(!format!("{settings:?}").contains("sk-test-123"));
}
