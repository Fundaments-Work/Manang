//! # manang-diagram
//!
//! Text-defined diagrams rendered to deterministic SVG (ADR 0011):
//!
//! - **Mermaid** through `merman` (pinned, no ELK layout),
//! - **Pikchr** through `pikru`,
//! - **ASCII diagrams** through `svgbob`.
//!
//! The crate is pure: no filesystem, clock, network or process access, so it
//! builds for `wasm32-unknown-unknown`. The same source and theme always
//! produce byte-identical SVG, coloured with the Fundaments.work tokens as
//! plain values (no CSS variables) so usvg and Typst draw it faithfully.

#![deny(missing_docs)]

pub mod color;
pub mod diagnostics;
pub mod engine;
pub mod error;
pub mod layout;
pub mod mermaid;
pub mod pikchr;
pub mod plugin;
pub mod svgbob;
pub mod svgxml;
pub mod theme;

pub use diagnostics::{Diagnostic, Severity};
pub use engine::{DiagramEngine, LanguageInfo};
pub use error::DiagramError;
pub use plugin::{DiagramPlugin, manifest, plugin};
pub use theme::Theme;
