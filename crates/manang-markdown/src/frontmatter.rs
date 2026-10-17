//! YAML and TOML frontmatter parsing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diagnostics::Diagnostic;

/// Parsed document metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Format identifier ("yaml" or "toml").
    pub format: String,
    /// Raw unparsed text of the frontmatter block.
    pub raw: String,
    /// Parsed structured data representation as JSON.
    pub data: Value,
}

fn yaml_to_json(yaml: &yaml_rust::Yaml) -> Value {
    match yaml {
        yaml_rust::Yaml::Real(s) => {
            if let Ok(n) = s.parse::<f64>() {
                serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::String(s.clone())
            }
        }
        yaml_rust::Yaml::Integer(i) => Value::Number((*i).into()),
        yaml_rust::Yaml::String(s) => Value::String(s.clone()),
        yaml_rust::Yaml::Boolean(b) => Value::Bool(*b),
        yaml_rust::Yaml::Array(arr) => Value::Array(arr.iter().map(yaml_to_json).collect()),
        yaml_rust::Yaml::Hash(h) => {
            let mut map = serde_json::Map::new();
            for (k, v) in h {
                let key_str = match k {
                    yaml_rust::Yaml::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                map.insert(key_str, yaml_to_json(v));
            }
            Value::Object(map)
        }
        yaml_rust::Yaml::Null | yaml_rust::Yaml::BadValue | yaml_rust::Yaml::Alias(_) => {
            Value::Null
        }
    }
}

/// Parses any leading YAML or TOML frontmatter block from markdown source.
/// Returns `(frontmatter, body_without_frontmatter, diagnostics)`.
pub fn parse_frontmatter(source: &str) -> (Option<Frontmatter>, &str, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();

    // YAML frontmatter: starts with `---` line
    if source.starts_with("---\n") || source.starts_with("---\r\n") {
        let start_offset = if source.starts_with("---\r\n") { 5 } else { 4 };
        let rest = &source[start_offset..];
        // Closing delimiter is `\n---` or `\n...`
        let mut close_idx = None;
        let mut delim_len = 0;

        for delim in [
            "\n---\n",
            "\n---\r\n",
            "\r\n---\r\n",
            "\r\n---\n",
            "\n...\n",
            "\n...\r\n",
        ] {
            if let Some(pos) = rest.find(delim) {
                if close_idx.is_none_or(|cur| pos < cur) {
                    close_idx = Some(pos);
                    delim_len = delim.len();
                }
            }
        }

        // Also check if rest ends exactly with `\n---` or `\n...`
        if close_idx.is_none() {
            if let Some(pos) = rest.rfind("\n---") {
                if rest[pos..].trim() == "---" {
                    close_idx = Some(pos);
                    delim_len = rest.len() - pos;
                }
            } else if let Some(pos) = rest.rfind("\n...") {
                if rest[pos..].trim() == "..." {
                    close_idx = Some(pos);
                    delim_len = rest.len() - pos;
                }
            }
        }

        if let Some(end) = close_idx {
            let raw = &rest[..end];
            let body = &rest[end + delim_len..];

            match yaml_rust::YamlLoader::load_from_str(raw) {
                Ok(docs) => {
                    let data = if let Some(first) = docs.first() {
                        yaml_to_json(first)
                    } else {
                        Value::Null
                    };
                    let fm = Frontmatter {
                        format: "yaml".to_string(),
                        raw: raw.to_string(),
                        data,
                    };
                    return (Some(fm), body, diagnostics);
                }
                Err(err) => {
                    diagnostics.push(Diagnostic::warning(
                        format!("failed to parse YAML frontmatter: {err}"),
                        Some(1),
                    ));
                    let fm = Frontmatter {
                        format: "yaml".to_string(),
                        raw: raw.to_string(),
                        data: Value::Null,
                    };
                    return (Some(fm), body, diagnostics);
                }
            }
        }
    }

    // TOML frontmatter: starts with `+++` line
    if source.starts_with("+++\n") || source.starts_with("+++\r\n") {
        let start_offset = if source.starts_with("+++\r\n") { 5 } else { 4 };
        let rest = &source[start_offset..];
        let mut close_idx = None;
        let mut delim_len = 0;

        for delim in ["\n+++\n", "\n+++\r\n", "\r\n+++\r\n", "\r\n+++\n"] {
            if let Some(pos) = rest.find(delim) {
                if close_idx.is_none_or(|cur| pos < cur) {
                    close_idx = Some(pos);
                    delim_len = delim.len();
                }
            }
        }

        if close_idx.is_none() {
            if let Some(pos) = rest.rfind("\n+++") {
                if rest[pos..].trim() == "+++" {
                    close_idx = Some(pos);
                    delim_len = rest.len() - pos;
                }
            }
        }

        if let Some(end) = close_idx {
            let raw = &rest[..end];
            let body = &rest[end + delim_len..];

            match toml::from_str::<Value>(raw) {
                Ok(data) => {
                    let fm = Frontmatter {
                        format: "toml".to_string(),
                        raw: raw.to_string(),
                        data,
                    };
                    return (Some(fm), body, diagnostics);
                }
                Err(err) => {
                    diagnostics.push(Diagnostic::warning(
                        format!("failed to parse TOML frontmatter: {err}"),
                        Some(1),
                    ));
                    let fm = Frontmatter {
                        format: "toml".to_string(),
                        raw: raw.to_string(),
                        data: Value::Null,
                    };
                    return (Some(fm), body, diagnostics);
                }
            }
        }
    }

    (None, source, diagnostics)
}
