//! Manang's self-healing plugin.
//!
//! Small, mechanical compile errors (a forgotten `\usepackage`, a Typst
//! package never imported, a `}` left open) have one obvious repair. This
//! crate turns a compile diagnostic plus the document text into that repair as
//! a list of edits, deterministically: no network, no model, no clock. Anything
//! it does not recognise it leaves alone (the agent's "Fix with AI" handles
//! the rest).
//!
//! The crate is pure Rust with no OS access, so it also builds for
//! `wasm32-unknown-unknown`. See [`plugin`] for the kernel commands and
//! [`suggest`] for the engine entry point.

mod engine;
mod error;
pub mod latex;
pub mod markdown;
pub mod plugin;
pub mod text;
mod types;
pub mod typst;

pub use engine::{apply_edits, suggest};
pub use error::HealError;
pub use plugin::{HealPlugin, manifest, plugin};
pub use types::{
    Confidence, DiagnosticIn, Edit, Engine, Finding, Fix, FixAll, ScalarRange, Suggestion,
};
