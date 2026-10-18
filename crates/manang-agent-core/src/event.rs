//! What a provider stream yields.

use crate::block::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token counts of one response. Cache fields are zero when unsupported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    /// Sums two usages (saturating).
    pub fn add(&self, other: &Usage) -> Usage {
        Usage {
            input: self.input.saturating_add(other.input),
            output: self.output.saturating_add(other.output),
            cache_read: self.cache_read.saturating_add(other.cache_read),
            cache_write: self.cache_write.saturating_add(other.cache_write),
        }
    }
}

/// Why the model stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    PauseTurn,
    Refusal,
    Other(String),
}

/// A failure reported by a provider or the transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderError {
    /// Provider error type or a transport code (`overloaded_error`, `http_401`...).
    pub code: String,
    pub message: String,
    /// Whether retrying the same request may succeed.
    pub retryable: bool,
    pub request_id: Option<String>,
}

/// One item of a provider stream. A stream ends after `Done` or `Error`.
///
/// Externally tagged on purpose: `Block` holds raw JSON, which serde cannot
/// buffer through an internally tagged enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEvent {
    /// The response began.
    Started {
        request_id: Option<String>,
        model: Option<String>,
    },
    /// Streamed assistant text.
    TextDelta { text: String },
    /// A complete block, exactly as the provider produced it (thinking
    /// blocks included). Append these to history to replay losslessly.
    Block { block: ContentBlock },
    /// A tool call, fully assembled and parsed.
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Final usage of the response.
    Usage { usage: Usage },
    /// The response finished.
    Done { stop_reason: StopReason },
    /// The attempt failed; terminal.
    Error { error: ProviderError },
}
