//! The request and outcome types shared by the plugin and the worker binary.
//!
//! These types carry no engine code. Tectonic itself lives in
//! [`crate::worker`], which is compiled only for the `manang-latex-worker`
//! binary (feature `engine`). The host application must never link Tectonic:
//! it statically bundles its own HarfBuzz, whose symbols override the system
//! copy that GTK and WebKit use and crash the app at startup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What the plugin asks the worker to do. Serialised as JSON on disk, never
/// carrying document bytes: those stay in files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRequest {
    /// Absolute path of the main `.tex` file.
    pub main: PathBuf,
    /// Directory `\input` and `\includegraphics` resolve against.
    pub root: PathBuf,
    /// Scratch directory that receives the PDF, log and SyncTeX files.
    pub out_dir: PathBuf,
    /// Bundle location; `None` selects Tectonic's default bundle.
    pub bundle: Option<String>,
    /// Holds `bundles/` and `formats/`.
    pub cache_dir: PathBuf,
    /// Refuse to touch the network (the bundle must already be cached).
    pub only_cached: bool,
    pub synctex: bool,
    /// Seconds since the Unix epoch embedded as the build date, so repeated
    /// compiles of the same input produce the same PDF.
    pub build_epoch: u64,
}

/// Why a compile produced no PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// The TeX support bundle could not be opened or fetched.
    Bundle,
    /// TeX reported errors; details are in the log.
    Tex,
    /// Anything else the engine refused to do.
    Engine,
}

/// The worker's verdict, written next to the outputs as `outcome.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerOutcome {
    pub failure: Option<FailureKind>,
    pub message: String,
    /// File stem of the outputs (`<stem>.pdf`, `<stem>.log`, ...).
    pub stem: String,
}

impl WorkerOutcome {
    #[cfg(feature = "engine")]
    pub(crate) fn failed(kind: FailureKind, message: String, stem: String) -> Self {
        Self {
            failure: Some(kind),
            message,
            stem,
        }
    }
}
