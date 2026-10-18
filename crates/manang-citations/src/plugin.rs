//! Kernel plugin interface for the citations engine.

use manang_kernel::{Context, Manifest, Payload, Plugin, PluginError};
use parking_lot::RwLock;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::Arc;

use crate::extract::extract_keys;
use crate::format::{format_entry, is_supported_style};
use crate::http::{BlockingHttpClient, HttpClient};
use crate::index::CitationIndex;
use crate::parser::parse_file;
use crate::verify::verify_entry;

const MANIFEST: &str = include_str!("../manifest.toml");

/// The citations plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Builds the default citations plugin instance.
pub fn plugin() -> impl Plugin {
    CitationsPlugin::new()
}

/// Builds a citations plugin with a custom HTTP client (e.g. for offline testing).
pub fn plugin_with_client(http_client: Arc<dyn HttpClient>) -> impl Plugin {
    CitationsPlugin::with_client(http_client)
}

/// Builds a citations plugin with a shared index and custom HTTP client.
pub fn plugin_with_store(
    index: Arc<CitationIndex>,
    http_client: Arc<dyn HttpClient>,
) -> impl Plugin {
    CitationsPlugin::with_store(index, http_client)
}

/// Exposes bibliographic management, CSL-JSON & BibTeX index, formatting, and verification as kernel commands.
///
/// Clones share the index and the configured settings.
#[derive(Clone)]
pub struct CitationsPlugin {
    index: Arc<CitationIndex>,
    http_client: Arc<dyn HttpClient>,
    /// `citations.style`: used when `citations.format` names no style.
    style: Arc<RwLock<String>>,
}

/// Setting id declared in `manifest.toml`.
const SETTING_STYLE: &str = "citations.style";
/// Built-in default of `citations.style`.
pub const DEFAULT_STYLE: &str = "apa";

fn default_style() -> Arc<RwLock<String>> {
    Arc::new(RwLock::new(DEFAULT_STYLE.to_owned()))
}

impl Default for CitationsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl CitationsPlugin {
    /// Creates a new citations plugin with default blocking HTTP client.
    pub fn new() -> Self {
        Self {
            index: Arc::new(CitationIndex::new()),
            http_client: Arc::new(BlockingHttpClient::new()),
            style: default_style(),
        }
    }

    /// Creates a citations plugin with a custom HTTP client.
    pub fn with_client(http_client: Arc<dyn HttpClient>) -> Self {
        Self {
            index: Arc::new(CitationIndex::new()),
            http_client,
            style: default_style(),
        }
    }

    /// Creates a citations plugin with a custom index and HTTP client.
    pub fn with_store(index: Arc<CitationIndex>, http_client: Arc<dyn HttpClient>) -> Self {
        Self {
            index,
            http_client,
            style: default_style(),
        }
    }

    /// The configured default citation style.
    pub fn style(&self) -> String {
        self.style.read().clone()
    }

    /// `citations.configure {values: {id: value}}`; all-or-nothing, unknown
    /// ids ignored.
    fn configure(&self, obj: &Map<String, Value>) -> Result<Value, PluginError> {
        check_allowed_keys(obj, &["values"])?;
        let values = obj
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| PluginError::Failed("`values` must be an object".to_owned()))?;
        let mut style = self.style();
        if let Some(value) = values.get(SETTING_STYLE) {
            let name = value.as_str().ok_or_else(|| {
                PluginError::Failed(format!("`{SETTING_STYLE}` must be a string"))
            })?;
            if !is_supported_style(name) {
                return Err(PluginError::Failed(format!(
                    "`{SETTING_STYLE}`: unsupported citation style `{name}`"
                )));
            }
            style = name.to_owned();
        }
        *self.style.write() = style.clone();
        Ok(json!({ SETTING_STYLE: style }))
    }

    /// Returns the underlying index.
    pub fn index(&self) -> &Arc<CitationIndex> {
        &self.index
    }

    /// Returns the underlying HTTP client.
    pub fn http_client(&self) -> &Arc<dyn HttpClient> {
        &self.http_client
    }
}

fn args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(PluginError::Failed("expected a JSON payload".to_owned())),
    }
}

fn check_allowed_keys(obj: &Map<String, Value>, allowed: &[&str]) -> Result<(), PluginError> {
    for k in obj.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(PluginError::Failed(format!("unexpected argument `{k}`")));
        }
    }
    Ok(())
}

fn failed(error: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(error.to_string())
}

impl Plugin for CitationsPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let a = args(input)?;
        let obj = a.as_object().ok_or_else(|| {
            PluginError::Failed("expected JSON object with named arguments".to_string())
        })?;

        match command {
            "citations.configure" => self.configure(obj).map(Payload::Json),
            "citations.load" => {
                check_allowed_keys(obj, &["paths", "path"])?;

                let mut paths = Vec::new();
                if let Some(arr) = obj.get("paths").and_then(Value::as_array) {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            paths.push(s.to_string());
                        } else {
                            return Err(PluginError::Failed(
                                "`paths` items must be strings".to_string(),
                            ));
                        }
                    }
                } else if let Some(s) = obj.get("path").and_then(Value::as_str) {
                    paths.push(s.to_string());
                } else {
                    return Err(PluginError::Failed(
                        "either `paths` (string[]) or `path` (string) is required".to_string(),
                    ));
                }

                let mut loaded_count = 0;
                let mut diagnostics = Vec::new();

                for p_str in paths {
                    let path = Path::new(&p_str);
                    match parse_file(path) {
                        Ok(entries) => {
                            loaded_count += self.index.insert_batch(entries);
                        }
                        Err(diag) => {
                            diagnostics.push(diag);
                        }
                    }
                }

                Ok(Payload::Json(json!({
                    "loaded": loaded_count,
                    "total": self.index.len(),
                    "errors": diagnostics,
                })))
            }

            "citations.search" => {
                check_allowed_keys(obj, &["query", "limit"])?;

                let query = obj
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::Failed("`query` must be a string".to_string()))?;

                let limit = match obj.get("limit") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v.as_u64().ok_or_else(|| {
                        PluginError::Failed("`limit` must be a positive integer".to_string())
                    })? as usize),
                };

                let results = self.index.search(query, limit);
                Ok(Payload::Json(json!({
                    "results": results,
                })))
            }

            "citations.get" => {
                check_allowed_keys(obj, &["key"])?;

                let key = obj
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::Failed("`key` must be a string".to_string()))?;

                let entry = self
                    .index
                    .get(key)
                    .ok_or_else(|| PluginError::Failed(format!("entry `{key}` not found")))?;

                serde_json::to_value(entry)
                    .map(Payload::Json)
                    .map_err(failed)
            }

            "citations.format" => {
                check_allowed_keys(obj, &["key", "style"])?;

                let key = obj
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::Failed("`key` must be a string".to_string()))?;

                let configured;
                let style = match obj.get("style").and_then(Value::as_str) {
                    Some(style) => style,
                    None => {
                        configured = self.style();
                        configured.as_str()
                    }
                };

                let entry = self
                    .index
                    .get(key)
                    .ok_or_else(|| PluginError::Failed(format!("entry `{key}` not found")))?;

                let formatted = format_entry(&entry, style).map_err(failed)?;

                Ok(Payload::Json(json!({
                    "key": key,
                    "style": style,
                    "formatted": formatted,
                })))
            }

            "citations.extract-keys" => {
                check_allowed_keys(obj, &["source", "syntax"])?;

                let source = obj
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::Failed("`source` must be a string".to_string()))?;

                let syntax = obj.get("syntax").and_then(Value::as_str).ok_or_else(|| {
                    PluginError::Failed(
                        "`syntax` must be a string (typst, latex, or markdown)".to_string(),
                    )
                })?;

                let keys = extract_keys(source, syntax).map_err(failed)?;

                Ok(Payload::Json(json!({
                    "keys": keys,
                })))
            }

            "citations.verify" => {
                check_allowed_keys(obj, &["key"])?;

                let key = obj
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::Failed("`key` must be a string".to_string()))?;

                let entry = self
                    .index
                    .get(key)
                    .ok_or_else(|| PluginError::Failed(format!("entry `{key}` not found")))?;

                let result = verify_entry(&entry, &*self.http_client);

                serde_json::to_value(result)
                    .map(Payload::Json)
                    .map_err(failed)
            }

            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
