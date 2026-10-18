//! Structured errors: every message starts with a `[code]` the UI and tests match on.
//!
//! Nothing here ever carries an API key: constructors take fixed text or user-visible names.

use manang_kernel::PluginError;

/// A failure with a stable code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("[{code}] {message}")]
pub struct AgentError {
    pub code: &'static str,
    pub message: String,
}

impl AgentError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new("storage", message)
    }

    pub fn disabled(message: impl Into<String>) -> Self {
        Self::new("disabled", message)
    }
}

impl From<AgentError> for PluginError {
    fn from(error: AgentError) -> Self {
        PluginError::Failed(error.to_string())
    }
}

impl From<manang_agent_core::CoreError> for AgentError {
    fn from(error: manang_agent_core::CoreError) -> Self {
        match error {
            manang_agent_core::CoreError::Policy(m) => Self::new("policy", m),
            other => Self::new("internal", other.to_string()),
        }
    }
}
