//! Kernel plugin interface for the figure engine.

use std::path::Path;
use std::sync::Arc;

use manang_kernel::{Context, Manifest, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::scene::Vec2;
use crate::store::{FigureId, FigureOp, FigureStore};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The figure plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Builds the default figure plugin instance.
pub fn plugin() -> impl Plugin {
    FigurePlugin::new()
}

/// Builds a figure plugin sharing an existing [`FigureStore`].
pub fn plugin_with_store(store: Arc<FigureStore>) -> impl Plugin {
    FigurePlugin::with_store(store)
}

/// Exposes a [`FigureStore`] as kernel commands.
#[derive(Clone)]
pub struct FigurePlugin {
    store: Arc<FigureStore>,
}

impl Default for FigurePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl FigurePlugin {
    /// Creates a new figure plugin backed by an empty figure store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(FigureStore::new()),
        }
    }

    /// Creates a figure plugin with a pre-existing store.
    pub fn with_store(store: Arc<FigureStore>) -> Self {
        Self { store }
    }

    /// Returns a reference to the underlying store.
    pub fn store(&self) -> &Arc<FigureStore> {
        &self.store
    }
}

fn args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(PluginError::Failed("expected a JSON payload".to_owned())),
    }
}

fn id(args: &Value) -> Result<Option<FigureId>, PluginError> {
    match args.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| PluginError::Failed("`id` must be an unsigned integer".to_owned())),
    }
}

fn number_f64(args: &Value, key: &str) -> Result<f64, PluginError> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be a number")))
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, PluginError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be a string")))
}

fn failed(error: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(error.to_string())
}

impl Plugin for FigurePlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let a = args(input)?;
        match command {
            "figure.create" => {
                let width = a.get("width").and_then(Value::as_f64);
                let height = a.get("height").and_then(Value::as_f64);
                match a.get("path") {
                    None | Some(Value::Null) => {
                        let id = self.store.create(width, height);
                        Ok(Payload::Json(json!({ "id": id })))
                    }
                    Some(_) => {
                        let path = string(&a, "path")?;
                        let (id, saved) = self
                            .store
                            .create_at(Path::new(path), width, height)
                            .map_err(failed)?;
                        Ok(Payload::Json(json!({
                            "id": id,
                            "json": saved.json,
                            "svg": saved.svg,
                        })))
                    }
                }
            }
            "figure.open" => {
                let path_str = string(&a, "path")?;
                let id = self.store.open(Path::new(path_str)).map_err(failed)?;
                Ok(Payload::Json(json!({ "id": id })))
            }
            "figure.save" => {
                let fig_id = id(&a)?;
                let path = a.get("path").and_then(Value::as_str).map(Path::new);
                let saved = self.store.save(fig_id, path).map_err(failed)?;
                serde_json::to_value(saved)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            "figure.close" => {
                let fig_id =
                    id(&a)?.ok_or_else(|| PluginError::Failed("`id` is required".to_owned()))?;
                let closed = self.store.close(fig_id).map_err(failed)?;
                Ok(Payload::Json(json!({ "closed": closed })))
            }
            "figure.list" => {
                let list = self.store.list();
                serde_json::to_value(list)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            "figure.apply" => {
                let fig_id = id(&a)?;
                let op_val = a
                    .get("op")
                    .ok_or_else(|| PluginError::Failed("missing `op` field".to_owned()))?;
                let op: FigureOp = serde_json::from_value(op_val.clone())
                    .map_err(|e| PluginError::Failed(format!("invalid `op`: {e}")))?;
                let res = self.store.apply(fig_id, op).map_err(failed)?;
                serde_json::to_value(res).map(Payload::Json).map_err(failed)
            }
            "figure.get" => {
                let fig_id = id(&a)?;
                let scene = self.store.get(fig_id).map_err(failed)?;
                serde_json::to_value(scene)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            // Stateless: render the scene the caller sends. Used on every save,
            // so it must not open (and retain) a document in the store.
            "figure.render-svg" => {
                let scene_val = a
                    .get("scene")
                    .ok_or_else(|| PluginError::Failed("missing `scene` field".to_owned()))?;
                let scene: crate::FigureScene = serde_json::from_value(scene_val.clone())
                    .map_err(|e| PluginError::Failed(format!("invalid `scene`: {e}")))?;
                let svg = crate::render_svg(&scene, &crate::SvgExportOptions::default());
                Ok(Payload::Binary(Arc::from(svg.into_bytes())))
            }
            "figure.export-svg" => {
                let fig_id = id(&a)?;
                let svg = self.store.export_svg(fig_id, None).map_err(failed)?;
                Ok(Payload::Binary(Arc::from(svg.into_bytes())))
            }
            "figure.hit-test" => {
                let fig_id = id(&a)?;
                let x = number_f64(&a, "x")?;
                let y = number_f64(&a, "y")?;
                let hit = self
                    .store
                    .hit_test(fig_id, Vec2::new(x, y))
                    .map_err(failed)?;
                let res = match hit {
                    Some(node) => json!({
                        "hit": true,
                        "node_id": node.id(),
                        "node": node,
                    }),
                    None => json!({
                        "hit": false,
                        "node_id": Value::Null,
                        "node": Value::Null,
                    }),
                };
                Ok(Payload::Json(res))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
