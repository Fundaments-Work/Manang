//! Shared fixtures: the in-house providers' recorded cassettes, and the two providers built
//! against the same mock server so every test can run both.
#![allow(dead_code)]

use super::MockServer;
use manang_agent_core::*;
use manang_agent_providers as inhouse;
use manang_agent_rig as rig;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

pub const A_TEXT: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/anthropic_text.sse");
pub const A_TOOL: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/anthropic_tool.sse");
pub const A_MID_ERROR: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/anthropic_midstream_error.sse");
pub const DS_TEXT: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/deepseek_text.sse");
pub const DS_REASONING: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/deepseek_reasoning.sse");
pub const DS_TOOLS: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/deepseek_tools.sse");
pub const DS_402: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/deepseek_402.json");
pub const DS_429: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/deepseek_429.json");
pub const OLLAMA: &str =
    include_str!("../../../manang-agent-providers/tests/cassettes/ollama_text.sse");

pub fn tool_spec() -> ToolSpec {
    ToolSpec {
        name: "figure.add_shape".into(),
        title: None,
        description: "Adds a shape".into(),
        input_schema: json!({"type": "object", "properties": {"kind": {"type": "string"}}}),
        output_schema: None,
        risk: RiskClass::Propose,
    }
}

pub fn fast_retry() -> rig::RetryPolicy {
    rig::RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_secs(5),
    }
}

fn fast_retry_inhouse() -> inhouse::RetryPolicy {
    inhouse::RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_secs(5),
    }
}

pub fn rig_http(policy: rig::HttpPolicy) -> rig::Http {
    rig::native::build_client(&rig::native::NativeConfig::default(), &policy).expect("client")
}

pub fn rig_policy() -> rig::HttpPolicy {
    rig::HttpPolicy {
        retry: fast_retry(),
        ..rig::HttpPolicy::default()
    }
}

/// Both implementations of the Anthropic provider, pointed at `server`.
pub fn anthropic_pair(server: &MockServer) -> (inhouse::AnthropicProvider, rig::RigProvider) {
    let mut config = inhouse::AnthropicConfig::new(inhouse::ApiKey::new("sk-test-123"));
    config.base_url = server.url();
    config.retry = fast_retry_inhouse();
    let transport =
        inhouse::ReqwestTransport::new(&inhouse::TransportConfig::default()).expect("transport");
    let a = inhouse::AnthropicProvider::new(config, Arc::new(transport));
    let mut settings = rig::AnthropicSettings::new(rig::ApiKey::new("sk-test-123"));
    settings.base_url = server.url();
    let b = rig::RigProvider::new(rig::Backend::Anthropic(settings), rig_http(rig_policy()));
    (a, b)
}

/// Both implementations for a DeepSeek preset (`reasoning_passback`).
pub fn deepseek_pair(server: &MockServer) -> (inhouse::OpenAiCompatProvider, rig::RigProvider) {
    let preset = preset_by_id("deepseek").expect("preset");
    let mut config = inhouse::OpenAiCompatConfig::from_preset(
        &preset,
        None,
        Some(inhouse::ApiKey::new("ds-key-1")),
    )
    .expect("config");
    config.base_url = format!("{}/v1", server.url());
    config.retry = fast_retry_inhouse();
    let transport =
        inhouse::ReqwestTransport::new(&inhouse::TransportConfig::default()).expect("transport");
    let a = inhouse::OpenAiCompatProvider::new(config, Arc::new(transport));
    let backend = rig::Backend::from_preset(
        &preset,
        Some(&format!("{}/v1", server.url())),
        Some(rig::ApiKey::new("ds-key-1")),
    )
    .expect("backend");
    let b = rig::RigProvider::new(backend, rig_http(rig_policy()));
    (a, b)
}

/// Ollama-style: no key, generic gateway.
pub fn ollama_pair(server: &MockServer) -> (inhouse::OpenAiCompatProvider, rig::RigProvider) {
    let mut config = inhouse::OpenAiCompatConfig::new("", None);
    config.base_url = format!("{}/v1", server.url());
    config.retry = fast_retry_inhouse();
    let transport =
        inhouse::ReqwestTransport::new(&inhouse::TransportConfig::default()).expect("transport");
    let a = inhouse::OpenAiCompatProvider::new(config, Arc::new(transport));
    let settings = rig::OpenAiSettings::new(&format!("{}/v1", server.url()), None);
    let b = rig::RigProvider::new(rig::Backend::OpenAiCompat(settings), rig_http(rig_policy()));
    (a, b)
}
