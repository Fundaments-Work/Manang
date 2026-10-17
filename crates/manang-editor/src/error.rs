use thiserror::Error;

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("no document with id {0}")]
    NoSuchDocument(u64),
    #[error("no document is active")]
    NoActiveDocument,
    #[error("range {from}..{to} is outside the document ({len} characters)")]
    OutOfRange { from: usize, to: usize, len: usize },
    #[error("document has no path; open or save it with a path first")]
    NoPath,
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}
