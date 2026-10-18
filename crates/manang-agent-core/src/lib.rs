//! Manang AI agent core.
//!
//! Provider-neutral types and pure logic only: no I/O, no async runtime, no
//! clock. It must stay buildable for `wasm32-unknown-unknown` so the web
//! version can reuse it. Network adapters live in `manang-agent-providers`.

pub mod block;
pub mod compact;
pub mod error;
pub mod event;
pub mod log;
pub mod mock;
pub mod names;
pub mod policy;
pub mod presets;
pub mod price;
pub mod provider;
pub mod redact;
pub mod request;
pub mod sse;

pub use block::{BlockKind, ContentBlock, Message, Role, ToolUseView};
pub use error::CoreError;
pub use event::{ProviderError, ProviderEvent, StopReason, Usage};
pub use log::{
    AuditEntry, AuditEvent, ReadLog, TranscriptEntry, append, compact_json, encode_line,
    payload_hash, read_all,
};
pub use mock::{Cassette, MockProvider};
pub use names::{ToolNameMap, from_wire, to_wire};
pub use policy::{EffectivePolicy, PolicyLayer};
pub use presets::{MaxTokensField, ProviderKind, ProviderPreset, preset_by_id, presets};
pub use price::{CostEstimate, Price, PriceTable};
pub use provider::{EventStream, Provider};
pub use redact::{Redaction, RedactionConfig, RedactionKind, RedactionSpan, redact};
pub use request::{CacheBreakpoint, ChatRequest, MAX_CACHE_BREAKPOINTS, RiskClass, ToolSpec};
pub use sse::{MAX_EVENT_BYTES, SseEvent, SseParser};
