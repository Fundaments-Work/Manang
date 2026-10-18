//! The kernel-facing command surface of the recovery store.

use std::path::PathBuf;

use manang_kernel::{Context, Manifest, ManifestError, Payload, Plugin, PluginError};
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::store::{RecoveryError, RecoveryStore, WriteRequest};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the plugin, keeping snapshots under `dir`
/// (`<app data>/manang/recovery`, resolved by the shell).
pub fn plugin(dir: impl Into<PathBuf>) -> impl Plugin {
    RecoveryPlugin::new(dir)
}

/// Implements [`Plugin`] for the `recovery.*` commands.
pub struct RecoveryPlugin {
    store: RecoveryStore,
}

impl RecoveryPlugin {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            store: RecoveryStore::new(dir),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectArgs {
    project: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileArgs {
    project: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscardArgs {
    project: String,
    #[serde(default)]
    path: Option<String>,
}

fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, PluginError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => {
            return Err(PluginError::Failed("expected a JSON payload".to_owned()));
        }
    };
    serde_json::from_value(value).map_err(|e| PluginError::Failed(format!("bad arguments: {e}")))
}

fn failed(e: RecoveryError) -> PluginError {
    PluginError::Failed(e.to_string())
}

fn json_payload<T: Serialize>(value: &T) -> Result<Payload, PluginError> {
    serde_json::to_value(value)
        .map(Payload::Json)
        .map_err(|e| PluginError::Failed(e.to_string()))
}

impl Plugin for RecoveryPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match command {
            "recovery.write" => {
                let req: WriteRequest = parse(input)?;
                json_payload(&self.store.write(req).map_err(failed)?)
            }
            "recovery.list" => {
                let a: ProjectArgs = parse(input)?;
                json_payload(&self.store.list(&a.project))
            }
            "recovery.read" => {
                let a: FileArgs = parse(input)?;
                match self.store.read(&a.project, &a.path).map_err(failed)? {
                    Some(snapshot) => json_payload(&snapshot),
                    None => Ok(Payload::Json(Value::Null)),
                }
            }
            "recovery.discard" => {
                let a: DiscardArgs = parse(input)?;
                let removed = self
                    .store
                    .discard(&a.project, a.path.as_deref())
                    .map_err(failed)?;
                Ok(Payload::Json(json!({ "removed": removed })))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
