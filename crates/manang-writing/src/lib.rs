//! Manang's writing-tools plugin.
//!
//! Formatting, spell checking, prose lint, symbol search, snippets and
//! statistics for Typst, LaTeX and Markdown documents. Everything works on
//! *prose*: [`mask::prose`] blanks out code, math, comments and commands
//! character-for-character, so offsets stay valid.

pub mod diag;
pub mod error;
pub mod format;
pub mod lint;
pub mod mask;
pub mod plugin;
pub mod snippets;
pub mod spell;
pub mod stats;
pub mod symbols;
pub mod syntax;
pub mod text;
pub mod words;

pub use diag::{DiagSeverity, Diagnostic, MAX_DIAGNOSTICS};
pub use error::WritingError;
pub use plugin::{WritingPlugin, manifest, plugin};
pub use syntax::Syntax;
pub use text::{CharRange, TextIndex};
