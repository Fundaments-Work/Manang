//! Manang's first-party search plugin.
//!
//! * fuzzy file-name finder over a cached, event-invalidated file index,
//! * project-wide text search on the `grep-*` crates with hard caps and
//!   cooperative cancellation,
//! * replace preview / atomic apply with content-hash guards,
//! * in-document find returning Unicode-scalar offsets.

pub mod budget;
mod error;
pub mod fuzzy;
pub mod index;
pub mod intext;
mod limits;
pub mod pattern;
mod plugin;
pub mod registry;
pub mod replace;
pub mod scan;
pub mod text;

pub use error::SearchError;
pub use limits::Limits;
pub use plugin::{NO_BUS_TTL, SearchConfig, SearchPlugin, manifest, plugin};
