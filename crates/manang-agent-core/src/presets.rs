//! Provider presets: data, not code. The user picks one (or a custom URL);
//! policy may lock the base URL. Facts were checked against each vendor's
//! documentation on 2026-10-18 unless the `notes` say otherwise.

use serde::{Deserialize, Serialize};

/// Which wire protocol a preset speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenAiCompat,
}

/// The request field that limits output tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTokensField {
    MaxTokens,
    MaxCompletionTokens,
}

/// A ready-made provider setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPreset {
    /// Stable id used in settings and policy (`allowed_providers`).
    pub id: String,
    pub display_name: String,
    pub kind: ProviderKind,
    /// `None` for the custom preset: the user supplies it.
    pub base_url: Option<String>,
    /// Known model ids, best default first. Empty with `fetch_models`.
    pub models: Vec<String>,
    /// List models from `GET {base_url}/models` instead.
    pub fetch_models: bool,
    pub key_required: bool,
    /// Environment variable the example CLI reads (never used by the app).
    pub key_env_var: Option<String>,
    /// Custom header for the key (name only); `None` means the standard
    /// header of the protocol (`Authorization: Bearer`, `x-api-key`).
    pub key_header: Option<String>,
    /// Send `reasoning_content` back in assistant messages (DeepSeek requires
    /// it when tools are used).
    pub reasoning_passback: bool,
    pub max_tokens_field: MaxTokensField,
    /// Only reachable on this machine (offline mode allows it).
    pub local: bool,
    pub notes: String,
}

// Rationale for the allow: a table-building helper; a parameter struct would only repeat
// the preset's own fields one level up.
#[allow(clippy::too_many_arguments)]
fn preset(
    id: &str,
    display_name: &str,
    kind: ProviderKind,
    base_url: Option<&str>,
    models: &[&str],
    key_required: bool,
    key_env_var: Option<&str>,
    local: bool,
    notes: &str,
) -> ProviderPreset {
    ProviderPreset {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        kind,
        base_url: base_url.map(str::to_owned),
        models: models.iter().map(|m| (*m).to_owned()).collect(),
        fetch_models: models.is_empty() && kind == ProviderKind::OpenAiCompat,
        key_required,
        key_env_var: key_env_var.map(str::to_owned),
        key_header: None,
        reasoning_passback: false,
        max_tokens_field: MaxTokensField::MaxTokens,
        local,
        notes: notes.to_owned(),
    }
}

/// All built-in presets, in display order.
pub fn presets() -> Vec<ProviderPreset> {
    use ProviderKind::{Anthropic, OpenAiCompat};
    let mut deepseek = preset(
        "deepseek",
        "DeepSeek",
        OpenAiCompat,
        Some("https://api.deepseek.com"),
        &["deepseek-flash", "deepseek-v4-pro"],
        true,
        Some("DEEPSEEK_API_KEY"),
        false,
        "Thinking is on by default and streams `reasoning_content`; it must be sent back when tools \
         are used (otherwise HTTP 400). The old names deepseek-chat and deepseek-reasoner were \
         scheduled for discontinuation on 2026-07-24 (current status unconfirmed). Prices vary by \
         time of day and cache hit.",
    );
    deepseek.reasoning_passback = true;
    let mut openai = preset(
        "openai",
        "OpenAI",
        OpenAiCompat,
        Some("https://api.openai.com/v1"),
        &[],
        true,
        Some("OPENAI_API_KEY"),
        false,
        "Models are listed from /models. Newer models may require max_completion_tokens (the API \
         reference could not be fetched; unverified).",
    );
    openai.max_tokens_field = MaxTokensField::MaxCompletionTokens;
    vec![
        preset(
            "anthropic",
            "Anthropic (Claude)",
            Anthropic,
            Some("https://api.anthropic.com"),
            &[
                "claude-sonnet-5-5",
                "claude-haiku-5-5",
                "claude-opus-5-5",
                "claude-fable-5-1",
            ],
            true,
            Some("ANTHROPIC_API_KEY"),
            false,
            "Default provider. Messages API with streaming, tool use, thinking and prompt caching.",
        ),
        openai,
        deepseek,
        preset(
            "openrouter",
            "OpenRouter",
            OpenAiCompat,
            Some("https://openrouter.ai/api/v1"),
            &[],
            true,
            Some("OPENROUTER_API_KEY"),
            false,
            "Gateway to many models; list them from /models.",
        ),
        preset(
            "groq",
            "Groq",
            OpenAiCompat,
            Some("https://api.groq.com/openai/v1"),
            &[],
            true,
            Some("GROQ_API_KEY"),
            false,
            "Rejects logprobs, logit_bias, messages[].name and n other than 1.",
        ),
        preset(
            "ollama",
            "Ollama (local)",
            OpenAiCompat,
            Some("http://localhost:11434/v1"),
            &[],
            false,
            None,
            true,
            "No key needed. Tool-calling quality depends on the local model.",
        ),
        preset(
            "llama_cpp",
            "llama.cpp server (local)",
            OpenAiCompat,
            Some("http://localhost:8080/v1"),
            &[],
            false,
            None,
            true,
            "llama-server's default port is 8080 (unverified; edit the URL if yours differs).",
        ),
        preset(
            "lm_studio",
            "LM Studio (local)",
            OpenAiCompat,
            Some("http://localhost:1234/v1"),
            &[],
            false,
            None,
            true,
            "LM Studio's default local server port (unverified; edit the URL if yours differs).",
        ),
        preset(
            "custom",
            "Custom OpenAI-compatible",
            OpenAiCompat,
            None,
            &[],
            false,
            None,
            false,
            "Any OpenAI-style server (vLLM, Together, a company gateway). Set the base URL, \
             optionally a key and a custom key header name.",
        ),
    ]
}

/// Looks a preset up by id.
pub fn preset_by_id(id: &str) -> Option<ProviderPreset> {
    presets().into_iter().find(|p| p.id == id)
}
