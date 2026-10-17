use std::time::Duration;

use thiserror::Error;

/// Everything that can go wrong before or around a compile. TeX errors in the
/// document are not `LatexError`s: they come back as diagnostics.
#[derive(Debug, Error)]
pub enum LatexError {
    #[error("`{0}` is not a valid argument: {1}")]
    BadArgument(&'static str, String),
    #[error("source path `{0}` must be relative and stay inside the project")]
    UnsafePath(String),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "the LaTeX worker `{0}` was not found; build it with `cargo build -p manang-latex` \
         or set MANANG_LATEX_WORKER"
    )]
    WorkerMissing(String),
    #[error("failed to start the LaTeX worker: {0}")]
    WorkerSpawn(std::io::Error),
    #[error("the LaTeX worker crashed: {0}")]
    WorkerCrashed(String),
    #[error("compilation exceeded the {0:?} limit and was killed")]
    Timeout(Duration),
    #[error(
        "the TeX support bundle is not cached ({0}); enable `allow_network` to download it once"
    )]
    BundleUnavailable(String),
    #[error("the engine failed: {0}")]
    Engine(String),
    #[error("no build is available; run `latex.compile` first")]
    NoBuild,
    #[error("SyncTeX data was not produced for this build")]
    NoSynctex,
    #[error("SyncTeX data is malformed: {0}")]
    BadSynctex(String),
    #[error("no SyncTeX match: {0}")]
    SynctexNoMatch(String),
}

impl LatexError {
    /// A stable machine-readable code, used as the `[code]` prefix of
    /// plugin error messages so the UI can branch without parsing prose.
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadArgument(..) => "bad_argument",
            Self::UnsafePath(_) => "unsafe_path",
            Self::Io { .. } => "io",
            Self::WorkerMissing(_) => "worker_missing",
            Self::WorkerSpawn(_) => "worker_spawn",
            Self::WorkerCrashed(_) => "worker_crashed",
            Self::Timeout(_) => "timeout",
            Self::BundleUnavailable(_) => "bundle_unavailable",
            Self::Engine(_) => "engine",
            Self::NoBuild => "no_build",
            Self::NoSynctex => "synctex_unavailable",
            Self::BadSynctex(_) => "synctex_malformed",
            Self::SynctexNoMatch(_) => "synctex_no_match",
        }
    }
}
