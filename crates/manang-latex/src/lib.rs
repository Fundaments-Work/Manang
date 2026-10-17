//! The LaTeX plugin: compiles documents with an embedded Tectonic engine.
//!
//! Tectonic runs in a short-lived `manang-latex-worker` process (see
//! [`engine`] for the protocol) so a runaway document can be killed on timeout, its global
//! state cannot leak between compiles, and all CPU work stays off the host's
//! async runtime. The plugin turns TeX and BibTeX logs into structured
//! [`diagnostics`] and answers [`synctex`] queries from the build's
//! `.synctex.gz`.
//!
//! Commands (see `manifest.toml`):
//! - `latex.compile` `{main, sources?, allow_network?, synctex?, timeout_ms?,
//!   build_epoch?}` returns `{ok, pages?, diagnostics, duration_ms, synctex}`.
//! - `latex.export-pdf` takes the same arguments (plus `recompile?`) and
//!   returns the PDF as `Payload::Binary`.
//! - `latex.synctex` `{main?, file, line}` returns `{positions}`;
//!   `{main?, page, x, y}` returns `{file, line}`.
//!
//! Failures that are not the document's fault are `PluginError::Failed`
//! messages of the form `[code] text`; see [`LatexError::code`].

pub mod diagnostics;
mod driver;
pub mod engine;
mod error;
mod plugin;
pub mod synctex;
pub mod system;
#[cfg(feature = "engine")]
pub mod worker;

pub use driver::{
    BackendChoice, Build, CompileSpec, LatexConfig, compile, compile_with, resolve_cache_dir,
};
pub use error::LatexError;
pub use plugin::{LatexPlugin, manifest, plugin};
pub use system::{Engine, TexTools};
