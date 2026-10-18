//! The interface every diagram language implements.

use serde::Serialize;

use crate::diagnostics::Diagnostic;
use crate::error::DiagramError;
use crate::theme::Theme;

/// What the frontend needs to know about a diagram language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LanguageInfo {
    /// Stable id used on the command boundary, e.g. `mermaid`.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// File extensions, with the leading dot.
    pub extensions: &'static [&'static str],
    /// Markdown fence info strings.
    pub fences: &'static [&'static str],
    /// The engine and its exact version, e.g. `merman 0.9.0`. Part of the
    /// cache key, so a dependency bump invalidates cached SVGs.
    pub engine: &'static str,
}

/// A text-to-SVG diagram engine.
///
/// Implementations are pure: no filesystem, network, clock or process
/// access, so the crate builds for `wasm32-unknown-unknown`. The same source
/// and theme always yield byte-identical SVG.
pub trait DiagramEngine: Send + Sync {
    /// Static description of the language.
    fn info(&self) -> &'static LanguageInfo;

    /// The language id (`info().id`).
    fn id(&self) -> &'static str {
        self.info().id
    }

    /// Reports problems in `source` without rendering it.
    fn check(&self, source: &str) -> Vec<Diagnostic>;

    /// Renders `source` to a standalone SVG document.
    fn render(&self, source: &str, theme: Theme) -> Result<String, DiagramError>;
}
