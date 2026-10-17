use thiserror::Error;

/// Why a Typst request could not be served. Typst *document* errors are not
/// failures of the plugin; they come back as structured diagnostics instead.
#[derive(Debug, Error)]
pub enum TypstError {
    #[error("invalid path `{path}`: {reason}")]
    BadPath { path: String, reason: String },
    #[error("`{0}` is outside the project root")]
    OutsideRoot(String),
    #[error("a relative main file needs a project root; pass an absolute path or configure one")]
    NoRoot,
    #[error("too many in-memory sources ({0}); the limit is {1}")]
    TooManySources(usize, usize),
    #[error("no successful compile yet; run `typst.compile` first")]
    NoDocument,
    #[error("page {page} is out of range; the document has {pages} page(s)")]
    PageOutOfRange { page: usize, pages: usize },
    #[error("could not start the compile thread pool: {0}")]
    Pool(String),
    #[error("PDF export failed: {0}")]
    Pdf(String),
    #[error("compile worker panicked")]
    WorkerPanicked,
}
