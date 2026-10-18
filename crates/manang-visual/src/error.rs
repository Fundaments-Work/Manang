use thiserror::Error;

/// A failure inside an adapter.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum AdapterError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("not supported: {0}")]
    Unsupported(String),
    #[error("adapter failed: {0}")]
    Internal(String),
}

/// A failure of the session host. `code()` is the stable IPC error code.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum VisualError {
    #[error("no visual session {0}")]
    NoSession(u32),
    #[error("{0}")]
    NoAdapter(String),
    #[error("{0}")]
    BadArgs(String),
    #[error("the session text and the editor disagree ({0}); reset the session")]
    Desync(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    TooLarge(String),
    #[error("{0}")]
    Failed(String),
}

impl VisualError {
    /// The `[code]` prefix the shell shows (ADR 0015 section 4).
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSession(_) => "visual.no-session",
            Self::NoAdapter(_) => "visual.no-adapter",
            Self::BadArgs(_) => "visual.bad-args",
            Self::Desync(_) => "visual.desync",
            Self::Unsupported(_) => "visual.unsupported",
            Self::TooLarge(_) => "visual.too-large",
            Self::Failed(_) => "visual.failed",
        }
    }
}

impl From<AdapterError> for VisualError {
    fn from(e: AdapterError) -> Self {
        match e {
            AdapterError::InvalidArgument(m) => Self::BadArgs(m),
            AdapterError::Unsupported(m) => Self::Unsupported(m),
            AdapterError::Internal(m) => Self::Failed(m),
        }
    }
}
