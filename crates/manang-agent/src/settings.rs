//! The user's settings, pushed by the settings sync to `agent.configure` (ADR 0007).
//!
//! The ids are declared in `manifest.toml`; this is their typed form.

use crate::error::AgentError;
use manang_agent_core::{PolicyLayer, ProviderPreset, preset_by_id};
use serde_json::{Map, Value};

pub const ENABLED: &str = "agent.enabled";
pub const PROVIDER: &str = "agent.provider";
pub const MODEL: &str = "agent.model";
pub const BASE_URL: &str = "agent.base-url";
pub const ENGINE: &str = "agent.client";
pub const OFFLINE: &str = "agent.offline";
pub const INCLUDE_THINKING: &str = "agent.include-thinking";
pub const CONTEXT_WINDOW: &str = "agent.context-window";
pub const REASONING_EFFORT: &str = "agent.reasoning-effort";
pub const USE_AGENTS_MD: &str = "agent.use-agents-md";
pub const REDACT_EMAILS: &str = "agent.redact-emails";
pub const REDACT_PATHS: &str = "agent.redact-paths";
pub const RETENTION_DAYS: &str = "agent.retention-days";

/// Which provider layer talks to the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Rig behind our adapter (the default).
    Rig,
    /// Manang's own adapters, kept as the fallback.
    Builtin,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rig => "rig",
            Self::Builtin => "builtin",
        }
    }
}

/// How hard a reasoning model thinks (DeepSeek's `reasoning_effort`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    Auto,
    Low,
    High,
}

impl Effort {
    /// The value sent to the provider for a session that starts with a slash command or not.
    pub fn resolve(self, command: bool) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Auto if command => "low",
            Self::Auto => "high",
        }
    }
}

/// The effective user settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub provider: String,
    /// Empty: the preset's first model.
    pub model: String,
    /// Empty: the preset's address.
    pub base_url: String,
    pub engine: Engine,
    pub offline: bool,
    /// Developer flag: stream the model's reasoning text to the UI.
    pub include_thinking: bool,
    /// Working window in tokens: compaction thresholds are shares of it.
    pub context_window: u64,
    /// `auto` (low for slash commands, high for open chat), `low` or `high`.
    pub reasoning_effort: Effort,
    /// Read `AGENTS.md` / `CLAUDE.md` as project instructions when `MANANG.md` is absent.
    pub use_agents_md: bool,
    pub redact_emails: bool,
    pub redact_paths: bool,
    pub retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: "anthropic".to_owned(),
            model: String::new(),
            base_url: String::new(),
            engine: Engine::Rig,
            offline: false,
            include_thinking: false,
            context_window: 100_000,
            reasoning_effort: Effort::Auto,
            use_agents_md: false,
            redact_emails: false,
            redact_paths: false,
            retention_days: 30,
        }
    }
}

fn bool_of(values: &Map<String, Value>, id: &str) -> Result<Option<bool>, AgentError> {
    match values.get(id) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(AgentError::invalid(format!("`{id}` must be true or false"))),
    }
}

fn text_of<'a>(values: &'a Map<String, Value>, id: &str) -> Result<Option<&'a str>, AgentError> {
    match values.get(id) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(AgentError::invalid(format!("`{id}` must be a string"))),
    }
}

fn int_of(values: &Map<String, Value>, id: &str) -> Result<Option<u64>, AgentError> {
    match values.get(id) {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| AgentError::invalid(format!("`{id}` must be a whole number"))),
    }
}

impl Settings {
    /// Applies `values` (`{id: value}`). All or nothing; unknown ids are ignored.
    pub fn applied(&self, values: &Map<String, Value>) -> Result<Self, AgentError> {
        let mut next = self.clone();
        if let Some(v) = bool_of(values, ENABLED)? {
            next.enabled = v;
        }
        if let Some(v) = text_of(values, PROVIDER)? {
            if preset_by_id(v).is_none() {
                return Err(AgentError::invalid(format!("unknown provider `{v}`")));
            }
            v.clone_into(&mut next.provider);
        }
        if let Some(v) = text_of(values, MODEL)? {
            v.trim().clone_into(&mut next.model);
        }
        if let Some(v) = text_of(values, BASE_URL)? {
            let url = v.trim();
            if !url.is_empty() {
                check_url(url)?;
            }
            url.clone_into(&mut next.base_url);
        }
        if let Some(v) = text_of(values, ENGINE)? {
            next.engine = match v {
                "rig" => Engine::Rig,
                "builtin" => Engine::Builtin,
                other => {
                    return Err(AgentError::invalid(format!("unknown engine `{other}`")));
                }
            };
        }
        if let Some(v) = bool_of(values, OFFLINE)? {
            next.offline = v;
        }
        if let Some(v) = bool_of(values, INCLUDE_THINKING)? {
            next.include_thinking = v;
        }
        if let Some(v) = int_of(values, CONTEXT_WINDOW)? {
            if !(8_000..=1_000_000).contains(&v) {
                return Err(AgentError::invalid(format!(
                    "`{CONTEXT_WINDOW}` must be between 8000 and 1000000"
                )));
            }
            next.context_window = v;
        }
        if let Some(v) = text_of(values, REASONING_EFFORT)? {
            next.reasoning_effort = match v {
                "auto" => Effort::Auto,
                "low" => Effort::Low,
                "high" => Effort::High,
                other => {
                    return Err(AgentError::invalid(format!(
                        "unknown reasoning effort `{other}`"
                    )));
                }
            };
        }
        if let Some(v) = bool_of(values, USE_AGENTS_MD)? {
            next.use_agents_md = v;
        }
        if let Some(v) = bool_of(values, REDACT_EMAILS)? {
            next.redact_emails = v;
        }
        if let Some(v) = bool_of(values, REDACT_PATHS)? {
            next.redact_paths = v;
        }
        if let Some(v) = int_of(values, RETENTION_DAYS)? {
            next.retention_days = u32::try_from(v.min(3650)).unwrap_or(3650);
        }
        Ok(next)
    }

    /// The preset in use.
    pub fn preset(&self) -> Result<ProviderPreset, AgentError> {
        preset_by_id(&self.provider)
            .ok_or_else(|| AgentError::invalid(format!("unknown provider `{}`", self.provider)))
    }

    /// The user layer of the policy merge: the user may switch the agent off and lower the limit.
    pub fn policy_layer(&self) -> PolicyLayer {
        PolicyLayer {
            enabled: Some(self.enabled),
            max_context_tokens: Some(self.context_window),
            ..PolicyLayer::default()
        }
    }
}

/// A base URL must be an `http(s)` URL with a host and no credentials.
pub fn check_url(url: &str) -> Result<(), AgentError> {
    let parsed = url::Url::parse(url)
        .map_err(|_| AgentError::invalid("the base URL is not a valid URL".to_owned()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AgentError::invalid("the base URL must be http or https"));
    }
    if parsed.host_str().is_none() {
        return Err(AgentError::invalid("the base URL has no host"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AgentError::invalid(
            "the base URL must not contain a user name or password",
        ));
    }
    Ok(())
}

/// Offline mode accepts `localhost` and loopback IP literals only. Other host names would need a
/// DNS lookup to judge, and the resolver of the HTTP client refuses non-loopback answers anyway.
pub fn check_offline(url: &str) -> Result<(), AgentError> {
    let blocked = |why: &str| AgentError::new("offline_blocked", why.to_owned());
    let parsed = url::Url::parse(url).map_err(|_| blocked("the base URL is not a valid URL"))?;
    match parsed.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            if d == "localhost" || d.ends_with(".localhost") {
                Ok(())
            } else {
                Err(blocked(
                    "offline mode only allows localhost or a loopback address as the provider address",
                ))
            }
        }
        Some(url::Host::Ipv4(ip)) if ip.is_loopback() => Ok(()),
        Some(url::Host::Ipv6(ip)) if ip.is_loopback() => Ok(()),
        _ => Err(blocked(
            "offline mode only allows localhost or a loopback address as the provider address",
        )),
    }
}
