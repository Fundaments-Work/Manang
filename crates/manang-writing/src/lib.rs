//! Manang's writing-tools plugin.
//!
//! Formatting, spell checking, prose lint, symbol search, snippets and
//! statistics for Typst, LaTeX and Markdown documents. Everything works on
//! *prose*: [`mask::prose`] blanks out code, math, comments and commands
//! character-for-character, so offsets stay valid.

pub mod diag;
pub mod error;
pub mod mask;
pub mod spell;
pub mod syntax;
pub mod text;
pub mod words;

pub use diag::{DiagSeverity, Diagnostic, MAX_DIAGNOSTICS};
pub use error::WritingError;
pub use syntax::Syntax;
pub use text::{CharRange, TextIndex};
