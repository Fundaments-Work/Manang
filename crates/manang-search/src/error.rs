//! Error types for the search plugin.

use std::io;

use manang_project::ProjectError;
use thiserror::Error;

/// Everything a search command can fail with.
#[derive(Debug, Error)]
pub enum SearchError {
    #[error("the search query is empty")]
    EmptyQuery,
    #[error("the query is {len} bytes, over the {limit} byte limit")]
    QueryTooLong { len: usize, limit: usize },
    #[error("invalid search pattern: {0}")]
    BadPattern(String),
    #[error("invalid glob `{glob}`: {reason}")]
    BadGlob { glob: String, reason: String },
    #[error("{given} globs given, at most {limit} are allowed")]
    TooManyGlobs { given: usize, limit: usize },
    #[error("the replacement is {len} bytes, over the {limit} byte limit")]
    ReplacementTooLong { len: usize, limit: usize },
    #[error("unknown or expired replace plan `{0}`")]
    UnknownPlan(String),
    #[error("could not start the search thread pool: {0}")]
    Pool(String),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

impl SearchError {
    #[allow(dead_code)]
    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
