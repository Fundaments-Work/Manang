use std::path::Path;
use std::sync::Arc;

use manang_kernel::{Context, Manifest, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::editor::{DocId, Editor};
use crate::error::EditorError;

const MANIFEST: &str = include_str!("../manifest.toml");

/// The editor's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Exposes an [`Editor`] as kernel commands.
pub struct EditorPlugin {
    editor: Arc<Editor>,
}

impl EditorPlugin {
    pub fn new(editor: Arc<Editor>) -> Self {
        Self { editor }
    }
}

fn args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(PluginError::Failed("expected a JSON payload".to_owned())),
    }
}

fn id(args: &Value) -> Result<Option<DocId>, PluginError> {
    match args.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| PluginError::Failed("`id` must be an unsigned integer".to_owned())),
    }
}

fn number(args: &Value, key: &str) -> Result<usize, PluginError> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be an unsigned integer")))
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, PluginError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be a string")))
}

fn failed(error: EditorError) -> PluginError {
    PluginError::Failed(error.to_string())
}

fn json_payload(value: Value) -> Payload {
    Payload::Json(value)
}

impl Plugin for EditorPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let a = args(input)?;
        let ed = &self.editor;
        match command {
            "editor.new" => Ok(json_payload(json!({ "id": ed.create() }))),
            "editor.open" => {
                let id = ed.open(Path::new(string(&a, "path")?)).map_err(failed)?;
                Ok(json_payload(json!({ "id": id })))
            }
            "editor.save" => {
                let path = a.get("path").and_then(Value::as_str).map(Path::new);
                ed.save(id(&a)?, path).map_err(failed)?;
                Ok(Payload::Empty)
            }
            "editor.activate" => {
                let target =
                    id(&a)?.ok_or_else(|| PluginError::Failed("`id` is required".to_owned()))?;
                ed.activate(target).map_err(failed)?;
                Ok(Payload::Empty)
            }
            "editor.insert" => {
                let revision = ed
                    .insert(id(&a)?, number(&a, "at")?, string(&a, "text")?)
                    .map_err(failed)?;
                Ok(json_payload(json!({ "revision": revision })))
            }
            "editor.delete" => {
                let revision = ed
                    .delete(id(&a)?, number(&a, "from")?, number(&a, "to")?)
                    .map_err(failed)?;
                Ok(json_payload(json!({ "revision": revision })))
            }
            "editor.text" => Ok(json_payload(Value::String(
                ed.text(id(&a)?).map_err(failed)?,
            ))),
            "editor.info" => {
                let info = ed.info(id(&a)?).map_err(failed)?;
                serde_json::to_value(info)
                    .map(json_payload)
                    .map_err(|e| PluginError::Failed(e.to_string()))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
