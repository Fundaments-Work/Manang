//! Tool-name mapping between internal ids and provider-safe names.
//!
//! Internal ids look like `figure.add_shape`; providers accept
//! `^[a-zA-Z0-9_-]{1,64}$`, so `.` becomes `__`. To keep the mapping
//! reversible, segments may not be empty, contain `__`, or start or end with
//! `_`.

use crate::error::CoreError;
use std::collections::BTreeMap;

/// Longest provider-side tool name.
pub const MAX_WIRE_LEN: usize = 64;

fn bad(name: &str, reason: &str) -> CoreError {
    CoreError::ToolName {
        name: name.to_owned(),
        reason: reason.to_owned(),
    }
}

/// `figure.add_shape` -> `figure__add_shape`.
pub fn to_wire(name: &str) -> Result<String, CoreError> {
    if name.is_empty() {
        return Err(bad(name, "empty"));
    }
    for segment in name.split('.') {
        if segment.is_empty() {
            return Err(bad(name, "empty segment"));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(bad(name, "segments allow only [A-Za-z0-9_-]"));
        }
        if segment.contains("__") || segment.starts_with('_') || segment.ends_with('_') {
            return Err(bad(name, "a segment may not contain `__` or edge `_`"));
        }
    }
    let wire = name.replace('.', "__");
    if wire.len() > MAX_WIRE_LEN {
        return Err(bad(name, "longer than 64 characters on the wire"));
    }
    Ok(wire)
}

/// `figure__add_shape` -> `figure.add_shape`. Does not check that the tool exists.
pub fn from_wire(wire: &str) -> String {
    wire.replace("__", ".")
}

/// A bidirectional map for one request's tool list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolNameMap {
    to_wire: BTreeMap<String, String>,
    to_internal: BTreeMap<String, String>,
}

impl ToolNameMap {
    /// Builds the map, rejecting invalid names and collisions.
    pub fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Self, CoreError> {
        let mut map = Self::default();
        for name in names {
            let wire = to_wire(name)?;
            if map
                .to_internal
                .insert(wire.clone(), name.to_owned())
                .is_some()
            {
                return Err(bad(name, "collides with another tool on the wire"));
            }
            map.to_wire.insert(name.to_owned(), wire);
        }
        Ok(map)
    }

    pub fn wire(&self, internal: &str) -> Option<&str> {
        self.to_wire.get(internal).map(String::as_str)
    }

    /// The internal name for a name the model used; `None` if the model
    /// invented a tool that was never offered.
    pub fn internal(&self, wire: &str) -> Option<&str> {
        self.to_internal.get(wire).map(String::as_str)
    }
}
