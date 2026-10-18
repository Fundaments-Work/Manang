//! Errors of the heal plugin.

use manang_kernel::PluginError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HealError {
    /// A request argument is missing or has the wrong type.
    #[error("bad argument `{name}`: {reason}")]
    BadArgument { name: &'static str, reason: String },
    /// The `engine` is not `typst`, `latex` or `markdown`.
    #[error("unknown engine `{0}`: expected `typst`, `latex` or `markdown`")]
    UnknownEngine(String),
}

impl HealError {
    pub(crate) fn bad_arg(name: &'static str, reason: impl Into<String>) -> Self {
        Self::BadArgument {
            name,
            reason: reason.into(),
        }
    }
}

impl From<HealError> for PluginError {
    fn from(error: HealError) -> Self {
        PluginError::Failed(error.to_string())
    }
}
