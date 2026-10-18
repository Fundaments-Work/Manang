use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use manang_kernel::{Context, Hook, Manifest, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::compiler::{CompileRequest, Compiler, CompilerConfig};
use crate::error::TypstError;

const MANIFEST: &str = include_str!("../manifest.toml");

/// The Typst plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the plugin. Fails only if the compile thread pool cannot start.
pub fn plugin(config: CompilerConfig) -> Result<impl Plugin, TypstError> {
    Ok(TypstPlugin::new(Arc::new(Compiler::new(config)?)))
}

/// Exposes a [`Compiler`] as kernel commands and the `on_lint` hook.
pub struct TypstPlugin {
    compiler: Arc<Compiler>,
}

impl TypstPlugin {
    pub fn new(compiler: Arc<Compiler>) -> Self {
        Self { compiler }
    }
}

fn failed(error: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(error.to_string())
}

fn json_args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(failed("expected a JSON payload")),
    }
}

fn parse_request(args: &Value) -> Result<CompileRequest, PluginError> {
    let main = args
        .get("main")
        .and_then(Value::as_str)
        .ok_or_else(|| failed("`main` must be a string path"))?
        .to_owned();
    let root = match args.get("root") {
        None | Some(Value::Null) => None,
        Some(v) => Some(PathBuf::from(
            v.as_str()
                .ok_or_else(|| failed("`root` must be a string"))?,
        )),
    };
    let mut sources = BTreeMap::new();
    match args.get("sources") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (path, text) in map {
                let text = text
                    .as_str()
                    .ok_or_else(|| failed(format!("source `{path}` must be a string")))?;
                sources.insert(path.clone(), text.to_owned());
            }
        }
        Some(_) => return Err(failed("`sources` must be an object of path to text")),
    }
    Ok(CompileRequest {
        main,
        root,
        sources,
    })
}

fn uint(args: &Value, key: &str) -> Result<usize, PluginError> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| failed(format!("`{key}` must be an unsigned integer")))
}

fn finite(args: &Value, key: &str) -> Result<f64, PluginError> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| failed(format!("`{key}` must be a finite number")))
}

impl Plugin for TypstPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let args = json_args(input)?;
        match command {
            "typst.compile" => {
                let report = self
                    .compiler
                    .compile(&parse_request(&args)?)
                    .map_err(failed)?;
                serde_json::to_value(report)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            "typst.render" => {
                let page = match args.get("page") {
                    None | Some(Value::Null) => 0,
                    Some(v) => v
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| failed("`page` must be an unsigned integer"))?,
                };
                self.compiler
                    .render(page)
                    .map(Payload::Binary)
                    .map_err(failed)
            }
            "typst.jump-from-click" => {
                let jump = self
                    .compiler
                    .jump_from_click(
                        uint(&args, "page")?,
                        finite(&args, "x")?,
                        finite(&args, "y")?,
                    )
                    .map_err(failed)?;
                serde_json::to_value(jump)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            "typst.jump-from-cursor" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| failed("`path` must be a string"))?;
                let positions = self
                    .compiler
                    .jump_from_cursor(path, uint(&args, "line")?, uint(&args, "column")?)
                    .map_err(failed)?;
                serde_json::to_value(positions)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            "typst.export-pdf" => self
                .compiler
                .export_pdf()
                .map(Payload::Binary)
                .map_err(failed),
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }

    /// `on_lint` compiles when the input names a `main` file (same shape as
    /// `typst.compile`), otherwise reports the diagnostics of the last compile.
    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        if hook != Hook::OnLint {
            return Ok(input);
        }
        let args = json_args(input)?;
        let diagnostics = if args.get("main").is_some() {
            self.compiler
                .compile(&parse_request(&args)?)
                .map_err(failed)?
                .diagnostics
        } else {
            self.compiler.diagnostics()
        };
        serde_json::to_value(diagnostics)
            .map(|d| Payload::Json(json!({ "source": "typst", "diagnostics": d })))
            .map_err(failed)
    }
}
