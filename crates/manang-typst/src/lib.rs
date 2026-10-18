//! The Typst compiler plugin.
//!
//! [`Compiler`] compiles a project directory overlaid with unsaved editor
//! text, incrementally, on a private Rayon pool, and caches the newest
//! document so page renders and PDF export never recompile. [`plugin`] wraps
//! it as a kernel plugin (`typst.compile`, `typst.render`, `typst.export-pdf`,
//! `typst.jump-from-click`, `typst.jump-from-cursor`, plus the `on_lint` hook).
//!
//! Compiling blocks the calling thread until the pool finishes. Callers on an
//! async runtime must reach it through `spawn_blocking`, never directly on a
//! Tokio worker.

mod compiler;
mod diagnostics;
mod error;
mod jump;
mod plugin;
mod world;

pub use compiler::{CompileReport, CompileRequest, Compiler, CompilerConfig, MAX_SOURCES};
pub use diagnostics::{CharRange, DiagSeverity, Diagnostic, MAX_DIAGNOSTICS};
pub use error::TypstError;
pub use jump::{PagePoint, SourceJump};
pub use plugin::{TypstPlugin, manifest, plugin};
pub use world::Environment;
