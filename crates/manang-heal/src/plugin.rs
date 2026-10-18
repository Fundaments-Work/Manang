//! Kernel plugin: `heal.suggest` and `heal.configure`.
//!
//! | command | input | output |
//! | --- | --- | --- |
//! | `heal.suggest` | `{engine, file?, text, diagnostics[]}` | `{fixes[], fix_all?, findings[], enabled}` |
//! | `heal.configure` | `{values: {setting-id: value}}` | the applied settings |
//!
//! `engine` is `typst`, `latex` or `markdown`. `diagnostics` are the compile
//! diagnostics as the compile commands return them (`message`, `line`,
//! `column`, `range`, `hints`). Edits are UTF-16 offsets of `text`.
//!
//! Settings: `heal.enabled` (off: `heal.suggest` returns nothing) and
//! `heal.auto-apply` (read by the shell, which applies certain fixes on
//! save; the plugin only stores and reports it).

use std::sync::atomic::{AtomicBool, Ordering};

use manang_kernel::{Context, Manifest, ManifestError, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::HealError;
use crate::engine::suggest;
use crate::types::{DiagnosticIn, Engine, Suggestion};

const MANIFEST: &str = include_str!("../manifest.toml");

const SETTING_ENABLED: &str = "heal.enabled";
const SETTING_AUTO_APPLY: &str = "heal.auto-apply";

/// The heal plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the plugin.
pub fn plugin() -> impl Plugin {
    HealPlugin::default()
}

/// Exposes the fix rules as kernel commands.
pub struct HealPlugin {
    enabled: AtomicBool,
    auto_apply: AtomicBool,
}

impl Default for HealPlugin {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            auto_apply: AtomicBool::new(false),
        }
    }
}

impl HealPlugin {
    fn suggest(&self, args: &Value) -> Result<Value, HealError> {
        let engine_name = args
            .get("engine")
            .and_then(Value::as_str)
            .ok_or_else(|| HealError::bad_arg("engine", "required string"))?;
        let engine = Engine::parse(engine_name)
            .ok_or_else(|| HealError::UnknownEngine(engine_name.to_owned()))?;
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| HealError::bad_arg("text", "required string"))?;
        let diagnostics: Vec<DiagnosticIn> = match args.get("diagnostics") {
            None | Some(Value::Null) => Vec::new(),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| HealError::bad_arg("diagnostics", e.to_string()))?,
        };
        let enabled = self.enabled.load(Ordering::Relaxed);
        let suggestion = if enabled {
            suggest(engine, text, &diagnostics)
        } else {
            Suggestion::default()
        };
        let mut out = serde_json::to_value(&suggestion)
            .map_err(|e| HealError::bad_arg("output", e.to_string()))?;
        if let Value::Object(map) = &mut out {
            map.insert("enabled".to_owned(), json!(enabled));
        }
        Ok(out)
    }

    fn configure(&self, args: &Value) -> Result<Value, HealError> {
        let values = args
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| HealError::bad_arg("values", "expected an object"))?;
        let mut enabled = self.enabled.load(Ordering::Relaxed);
        let mut auto_apply = self.auto_apply.load(Ordering::Relaxed);
        for (id, value) in values {
            let flag = |name: &'static str| {
                value.as_bool().ok_or_else(|| {
                    HealError::bad_arg("values", format!("`{name}` must be a boolean"))
                })
            };
            match id.as_str() {
                SETTING_ENABLED => enabled = flag(SETTING_ENABLED)?,
                SETTING_AUTO_APPLY => auto_apply = flag(SETTING_AUTO_APPLY)?,
                _ => {}
            }
        }
        self.enabled.store(enabled, Ordering::Relaxed);
        self.auto_apply.store(auto_apply, Ordering::Relaxed);
        Ok(json!({ SETTING_ENABLED: enabled, SETTING_AUTO_APPLY: auto_apply }))
    }
}

impl Plugin for HealPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let args = match input {
            Payload::Json(v) => v,
            Payload::Empty => Value::Null,
            Payload::Binary(_) => {
                return Err(HealError::bad_arg("input", "expected a JSON payload").into());
            }
        };
        let out = match command {
            "heal.suggest" => self.suggest(&args)?,
            "heal.configure" => self.configure(&args)?,
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        };
        Ok(Payload::Json(out))
    }
}
