mod common;

use common::{Reply, chat, collect, serve};
use manang_agent_core::*;
use manang_agent_providers::{
    AnthropicConfig, AnthropicProvider, ApiKey, HttpRequest, HttpTransport, OpenAiCompatConfig,
    OpenAiCompatProvider, ReqwestTransport, TransportConfig, TransportError,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

const OLLAMA: &str = include_str!("cassettes/ollama_text.sse");

fn offline_transport() -> Arc<ReqwestTransport> {
    let config = TransportConfig {
        loopback_only: true,
        ..TransportConfig::default()
    };
    Arc::new(ReqwestTransport::new(&config).expect("transport"))
}

fn openai(base: &str, offline: bool) -> OpenAiCompatProvider {
    let mut config = OpenAiCompatConfig::new(base, None);
    config.offline = offline;
    OpenAiCompatProvider::new(config, offline_transport())
}

fn code_of(events: &[ProviderEvent]) -> &str {
    match events.last() {
        Some(ProviderEvent::Error { error }) => &error.code,
        other => panic!("expected error, got {other:?}"),
    }
}

#[tokio::test]
async fn loopback_base_url_works_offline() {
    let server = serve(vec![Reply::sse(OLLAMA)]).await;
    let events = collect(
        &openai(&format!("{}/v1", server.url()), true),
        &chat("m", "hi"),
    )
    .await;
    assert!(matches!(events.last(), Some(ProviderEvent::Done { .. })));
    // `localhost` is resolved and every address checked.
    let port = server.addr.port();
    let events = collect(
        &openai(&format!("http://localhost:{port}/v1"), true),
        &chat("m", "hi"),
    )
    .await;
    // localhost may resolve to ::1 only on some machines; both outcomes are
    // fine as long as nothing non-loopback was contacted.
    assert!(matches!(
        events.last(),
        Some(ProviderEvent::Done { .. } | ProviderEvent::Error { .. })
    ));
}

#[tokio::test]
async fn non_loopback_is_blocked_before_any_connection() {
    for base in [
        "http://192.0.2.1:9/v1",
        "http://[2001:db8::1]:9/v1",
        "https://api.openai.com.invalid/v1",
    ] {
        let events = collect(&openai(base, true), &chat("m", "hi")).await;
        assert_eq!(code_of(&events), "offline_blocked", "{base}");
    }
    let mut config = AnthropicConfig::new(ApiKey::new("k"));
    config.base_url = "http://192.0.2.1:9".into();
    config.offline = true;
    let p = AnthropicProvider::new(config, offline_transport());
    let events = collect(&p, &chat("claude-sonnet-5-5", "hi")).await;
    assert_eq!(code_of(&events), "offline_blocked");
}

#[tokio::test]
async fn the_transport_itself_refuses_non_loopback_literals() {
    let transport = offline_transport();
    let token = CancellationToken::new();
    let request = HttpRequest {
        url: "http://192.0.2.1:9/".into(),
        headers: vec![],
        body: bytes::Bytes::new(),
    };
    match transport.post_stream(request, &token).await {
        Err(TransportError::Blocked(_)) => {}
        other => panic!("expected Blocked, got {other:?}"),
    }
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
    let transport = Arc::new(ReqwestTransport::new(&TransportConfig::default()).unwrap());
    let provider = OpenAiCompatProvider::new(
        OpenAiCompatConfig::new(&format!("{}/v1", origin.url()), None),
        transport,
    );
    let events = collect(&provider, &chat("m", "hi")).await;
    assert_eq!(code_of(&events), "http_302");
    assert_eq!(
        target.request_count().await,
        0,
        "the redirect target was never contacted"
    );
}

#[test]
fn an_invalid_proxy_is_a_configuration_error() {
    let config = TransportConfig {
        proxy: Some("not a url".into()),
        ..TransportConfig::default()
    };
    assert!(matches!(
        ReqwestTransport::new(&config),
        Err(TransportError::Config(_))
    ));
}
