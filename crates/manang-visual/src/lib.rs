//! Core of Manang's Visual editor (ADR 0015).
//!
//! * [`adapter`]: the format-adapter traits (the Rust shape of `wit/format.wit`).
//! * [`table`]: the frozen binary span-table encoding.
//! * [`session`]: stateful sessions (rope + adapter parse state) and the host.
//! * [`text`]: helpers adapters share to plan edits.
//!
//! No OS dependencies: this crate compiles to `wasm32-unknown-unknown`, so the
//! same host can run in a browser for the planned web version.

pub mod adapter;
pub mod error;
pub mod kind;
pub mod registry;
pub mod session;
pub mod table;
pub mod text;

pub use adapter::{
    AdapterInfo, AdapterSession, CommandState, EditPlan, FormatAdapter, OutlineItem, RawSpan,
    TextEdit,
};
pub use error::{AdapterError, VisualError};
pub use registry::AdapterRegistry;
pub use session::{
    Change, ChangeOut, DirtyRange, EditOutcome, MAX_DOC_BYTES, MAX_SESSIONS, OpenInfo, OutlineOut,
    PlanOut, Session, SessionHost,
};
pub use table::{Span, SpanTable, TableError};
