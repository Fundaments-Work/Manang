//! Manang AI agent: a Rig-backed provider.
//!
//! [`RigProvider`] implements `manang_agent_core::Provider` on top of Rig's `Model::stream`.
//! Rig (`rig-core`, pinned exactly) is an implementation detail: no Rig type appears in this
//! crate's public API, so swapping it out, or going back to the in-house adapters, touches
//! nothing else.
//!
//! What stays ours: the HTTP policy ([`PolicyHttp`]: retry rules, loopback-only mode, no
//! redirects, no ambient proxy, audit and redaction through [`RequestTap`]), the lossless block
//! model, tool-name mapping, error codes and the event vocabulary.

mod decode;
mod encode;
mod error;
mod http;
#[cfg(not(target_family = "wasm"))]
pub mod native;
mod provider;
pub mod retry;

pub use http::{Blocked, HttpPolicy, PolicyHttp, RequestTap, ensure_loopback_literal, is_loopback};
pub use provider::{
    AnthropicSettings, ApiKey, Backend, Http, OpenAiSettings, Preview, RigProvider,
};
pub use retry::RetryPolicy;
