//! Data crossing the plugin boundary.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A command or hook argument or result.
///
/// Small structured data travels as JSON. Large artefacts (PDFs, SVGs) travel
/// as shared binary buffers so they are never base64-encoded or copied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Payload {
    Empty,
    Json(Value),
    #[serde(skip)]
    Binary(Arc<[u8]>),
}
