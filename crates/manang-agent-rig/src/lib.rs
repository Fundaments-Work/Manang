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

// Rationale: Rig's `http_client::Error` and `ProviderError` are 128 to 224 bytes and are the
// error types of the traits we implement; Rig's own crates allow this lint for the same reason.
#![allow(clippy::result_large_err)]

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
