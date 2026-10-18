//! # manang-markdown
//!
//! Markdown parser, XSS-sanitized HTML renderer, outline extractor, and Typst converter
//! for the Manang scientific authoring suite.
//!
//! Features:
//! - Full support for tables, footnotes, task lists, strikethrough, and smart punctuation via `pulldown-cmark`.
//! - YAML (`---`) and TOML (`+++`) frontmatter extraction and parsing.
//! - Inline (`$...$`) and display (`$$...$$`) math preservation with KaTeX/MathJax-compatible wrapper elements.
//! - Strict XSS sanitization eliminating `javascript:` links and raw script tags unless explicitly permitted by an allowlist.
//! - Document outline extraction with 1-based line numbers and character offsets.
//! - High-fidelity Markdown to Typst converter reporting unsupported constructs as diagnostics.

#![deny(missing_docs)]

pub mod diagnostics;
pub mod diagrams;
pub mod error;
pub mod frontmatter;
pub mod html;
pub mod outline;
pub mod plugin;
pub mod typst;
pub mod visual;

pub use diagnostics::Diagnostic;
pub use diagrams::{DiagramFence, diagram_fences, fence_key, fence_language};
pub use error::MarkdownError;
pub use frontmatter::{Frontmatter, parse_frontmatter};
pub use html::{
    RenderOptions, diagram_placeholder, escape_html, is_unsafe_url, render_to_html,
    render_to_html_at,
};
pub use outline::{Heading, extract_outline};
pub use plugin::{MarkdownPlugin, manifest, plugin};
pub use typst::{FenceFigure, markdown_to_typst, markdown_to_typst_with};
