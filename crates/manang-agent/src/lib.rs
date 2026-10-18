//! Manang AI agent: the first-party plugin `agent`.
//!
//! It orchestrates a provider (Rig by default, the in-house adapters as a fallback), the tools
//! plugins declare (`Kernel::agent_tools`), approvals, proposals and storage. The frontend
//! applies every proposal as one editor transaction; this crate never writes a document.
//!
//! See `docs/plans/2026-10-18-ai-agent.md`.

mod context;
mod error;
mod plugin;
mod prompt;
mod provider;
mod risk;
mod session;
mod settings;
mod state;
mod storage;
mod text;
mod tools;

pub use context::{ChipInput, ChipKind, Diagnostic, ResolvedChip};
pub use error::AgentError;
pub use plugin::{AgentOptions, AgentPlugin, manifest, plugin};
pub use prompt::Feature;
pub use provider::{PreviewFuture, ProviderFactory, ProviderSpec, RealFactory, WirePreview};
pub use risk::{Decision, RiskTable, ToolInfo};
pub use settings::{Engine, Settings};
pub use state::Limits;
pub use storage::{SessionMeta, SessionStore, project_agent_dir};
pub use text::{estimate_tokens, markup_problem, outline, utf16_len};
