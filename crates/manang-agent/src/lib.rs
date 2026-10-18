//! Manang AI agent: the first-party plugin `agent`.
//!
//! It orchestrates a provider (Rig by default, the in-house adapters as a fallback), the tools
//! plugins declare (`Kernel::agent_tools`), approvals, proposals and storage. The frontend
//! applies every proposal as one editor transaction; this crate never writes a document.
//!
//! See `docs/plans/2026-10-18-agent-v2.md` (the design) and `docs/ai/using-the-agent.md`.

mod commands;
mod context;
mod edit;
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

pub use commands::{SlashCommand, builtins as builtin_commands};
pub use context::{Diagnostic, ResolvedPart, TurnInput};
pub use error::AgentError;
pub use plugin::{AgentOptions, AgentPlugin, manifest, plugin};
pub use prompt::SYSTEM_PROMPT;
pub use provider::{PreviewFuture, ProviderFactory, ProviderSpec, RealFactory, WirePreview};
pub use risk::{Decision, RiskTable, ToolInfo};
pub use settings::{Effort, Engine, Settings};
pub use state::Limits;
pub use storage::{SessionMeta, SessionStore, project_agent_dir};
pub use text::{
    balance_problem, estimate_tokens, markup_problem, markup_problem_asked, outline, pair_problem,
    utf16_len,
};
