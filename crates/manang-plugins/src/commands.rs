//! The first-party `plugins` plugin: the manager's commands, so the shell and
//! the agent reach it through the kernel like any other feature.
//!
//! Errors are `[code] message`; the code is stable (see
//! [`ManagerError::code`]), the message is for people.

use std::path::Path;
use std::sync::{Arc, Weak};

use manang_kernel::manifest::Permissions;
use manang_kernel::{Context, Manifest, ManifestError, Payload, Plugin, PluginError};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::ManagerError;
use crate::manager::PluginManager;
use crate::marketplace::{Marketplace, MarketplaceConfig};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The `plugins` plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the `plugins` plugin over `manager`. It holds the manager weakly:
/// the manager owns the kernel that owns this plugin.
pub fn plugin(manager: &Arc<PluginManager>) -> PluginsPlugin {
    plugin_with(manager, MarketplaceConfig::default())
}

/// [`plugin`] with the marketplace pointed at a specific registry (the app
/// gives it a cache folder; tests give it a local server and a test key).
pub fn plugin_with(manager: &Arc<PluginManager>, market: MarketplaceConfig) -> PluginsPlugin {
    PluginsPlugin {
        manager: Arc::downgrade(manager),
        market: Marketplace::new(market),
    }
}

pub struct PluginsPlugin {
    manager: Weak<PluginManager>,
    market: Marketplace,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApproveArgs {
    name: String,
    permissions: Permissions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectArgs {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

fn coded(code: &str, message: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(format!("[{code}] {message}"))
}

fn failed(e: ManagerError) -> PluginError {
    coded(e.code(), e)
}

fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, PluginError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => return Err(coded("bad_arguments", "expected a JSON payload")),
    };
    serde_json::from_value(value).map_err(|e| coded("bad_arguments", e))
}

/// The payload as a JSON object (empty for none).
fn args_json(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(Value::Null) | Payload::Empty => Ok(json!({})),
        Payload::Json(v) => Ok(v),
        Payload::Binary(_) => Err(coded("bad_arguments", "expected a JSON payload")),
    }
}

fn json_out<T: serde::Serialize>(value: &T) -> Result<Payload, PluginError> {
    serde_json::to_value(value)
        .map(Payload::Json)
        .map_err(|e| coded("serialize", e))
}

impl Plugin for PluginsPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let manager = self
            .manager
            .upgrade()
            .ok_or_else(|| coded("shutting_down", "the plugin manager has stopped"))?;
        match command {
            "plugins.list" => {
                parse::<NoArgs>(input)?;
                json_out(&manager.list_with_warnings())
            }
            "plugins.inspect" => {
                let a: PathArgs = parse(input)?;
                json_out(&manager.inspect(Path::new(&a.path)).map_err(failed)?)
            }
            "plugins.install" => {
                let a: PathArgs = parse(input)?;
                json_out(&manager.install(Path::new(&a.path)).map_err(failed)?)
            }
            "plugins.approve" => {
                let a: ApproveArgs = parse(input)?;
                json_out(&manager.approve(&a.name, &a.permissions).map_err(failed)?)
            }
            "plugins.enable" => {
                let a: NameArgs = parse(input)?;
                json_out(&manager.enable(&a.name).map_err(failed)?)
            }
            "plugins.disable" => {
                let a: NameArgs = parse(input)?;
                json_out(&manager.disable(&a.name).map_err(failed)?)
            }
            "plugins.uninstall" => {
                let a: NameArgs = parse(input)?;
                json_out(&manager.uninstall(&a.name).map_err(failed)?)
            }
            "plugins.reload" => {
                let a: NameArgs = parse(input)?;
                json_out(&manager.reload_plugin(&a.name).map_err(failed)?)
            }
            "plugins.permissions" => {
                let a: NameArgs = parse(input)?;
                json_out(&manager.permissions(&a.name).map_err(failed)?)
            }
            "plugins.contributions" => {
                parse::<NoArgs>(input)?;
                let kernel = &manager.kernel;
                Ok(Payload::Json(json!({
                    "format_adapters": kernel.format_adapters(),
                    "editors": kernel.editors(),
                    "toolbars": kernel.toolbars(),
                })))
            }
            "plugins.configure" => self
                .market
                .configure(&args_json(input)?)
                .map(Payload::Json)
                .map_err(failed),
            "plugins.registry-status" => {
                parse::<NoArgs>(input)?;
                Ok(Payload::Json(self.market.status(&manager)))
            }
            "plugins.browse" => self
                .market
                .browse(&manager, &args_json(input)?)
                .map(Payload::Json)
                .map_err(failed),
            "plugins.detail" => self
                .market
                .detail(&manager, &args_json(input)?)
                .map(Payload::Json)
                .map_err(failed),
            "plugins.check-updates" => self
                .market
                .check_updates(&manager, &args_json(input)?)
                .map(Payload::Json)
                .map_err(failed),
            "plugins.install-from-registry" => self
                .market
                .install(&manager, &args_json(input)?)
                .map(Payload::Json)
                .map_err(failed),
            "plugins.cancel-install" => {
                parse::<NoArgs>(input)?;
                self.market.cancel();
                Ok(Payload::Json(json!({})))
            }
            "plugins.use-project" => {
                let a: ProjectArgs = parse(input)?;
                let changed = manager
                    .set_project(a.path.as_deref().map(Path::new))
                    .map_err(failed)?;
                Ok(Payload::Json(json!({ "changed": changed })))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
