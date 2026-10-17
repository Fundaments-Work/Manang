//! # manang-buffer
//!
//! A high-performance, Rope-based text buffer for the Manang scientific authoring suite.
//!
//! Powered by [`ropey`], this crate provides sub-millisecond document manipulation,
//! line/column to character offset mappings, UTF-8 byte offset conversions, and an
//! undo/redo history with nested transaction support.

#![deny(missing_docs)]

mod buffer;
mod error;
mod history;
mod position;

pub use buffer::DocumentBuffer;
pub use error::BufferError;
pub use history::{Edit, History, Transaction};
pub use position::Position;
