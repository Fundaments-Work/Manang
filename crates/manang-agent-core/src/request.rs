//! Provider-neutral request types.

use crate::block::Message;
use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Providers allow at most this many cache breakpoints.
pub const MAX_CACHE_BREAKPOINTS: usize = 4;

/// How dangerous a tool is. Declared in the manifest and enforced by the
/// host; never taken from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// Reads in-scope data.
    Read,
    /// Returns a proposal the user must accept.
    Propose,
    /// Reaches the network.
    Network,
    /// Runs a process.
    Exec,
}

/// A tool the model may call (MCP-shaped).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Internal id, `<plugin>.<command>`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub risk: RiskClass,
}

/// Where a cache breakpoint goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheBreakpoint {
    /// After the tool definitions.
    Tools,
    /// After the system prompt.
    System,
    /// After the last block of `messages[index]`.
    Message(usize),
}

/// A chat request. History is append-only: adapters must not reorder or edit it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache_breakpoints: Vec<CacheBreakpoint>,
}

impl ChatRequest {
    /// Checks the invariants every adapter relies on.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.model.trim().is_empty() {
            return Err(CoreError::InvalidRequest("model is empty".into()));
        }
        if self.max_tokens == 0 {
            return Err(CoreError::InvalidRequest("max_tokens is zero".into()));
        }
        if self.messages.is_empty() {
            return Err(CoreError::InvalidRequest("no messages".into()));
        }
        if self.cache_breakpoints.len() > MAX_CACHE_BREAKPOINTS {
            return Err(CoreError::InvalidRequest(format!(
                "{} cache breakpoints, at most {MAX_CACHE_BREAKPOINTS}",
                self.cache_breakpoints.len()
            )));
        }
        for bp in &self.cache_breakpoints {
            if let CacheBreakpoint::Message(i) = bp
                && *i >= self.messages.len()
            {
                return Err(CoreError::InvalidRequest(format!(
                    "cache breakpoint on missing message {i}"
                )));
            }
        }
        Ok(())
    }
}
