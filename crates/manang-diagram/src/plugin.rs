//! Kernel plugin interface: `diagram.languages`, `diagram.check`,
//! `diagram.render`, `diagram.key`, `diagram.layout`.

use std::sync::Arc;

use manang_kernel::{Context, Manifest, Payload, Plugin, PluginError};
use serde_json::{Value, json};

use crate::diagnostics::Diagnostic;
use crate::engine::{DiagramEngine, LanguageInfo};
use crate::error::DiagramError;
use crate::mermaid::MermaidEngine;
use crate::pikchr::PikchrEngine;
use crate::svgbob::SvgbobEngine;
use crate::theme::Theme;

const MANIFEST: &str = include_str!("../manifest.toml");

/// Bumped when this crate changes its output for unchanged inputs (new
/// palette, new postprocessing), so cached SVGs are invalidated.
const OUTPUT_REVISION: &str = "1";

/// The diagram plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Builds the diagram plugin with every built-in engine.
pub fn plugin() -> DiagramPlugin {
    DiagramPlugin::new()
}

/// Dispatches diagram commands to the engine for each language.
#[derive(Clone)]
pub struct DiagramPlugin {
    engines: Arc<[Box<dyn DiagramEngine>]>,
}

impl Default for DiagramPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DiagramPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<_> = self.engines.iter().map(|e| e.id()).collect();
        f.debug_struct("DiagramPlugin")
            .field("engines", &ids)
            .finish()
    }
}

impl DiagramPlugin {
    /// Creates the plugin with Mermaid, Pikchr and svgbob.
    pub fn new() -> Self {
        Self {
            engines: Arc::new([
                Box::new(MermaidEngine::new()) as Box<dyn DiagramEngine>,
                Box::new(PikchrEngine::new()),
                Box::new(SvgbobEngine::new()),
            ]),
        }
    }

    /// Every supported language, in a fixed order.
    pub fn languages(&self) -> Vec<&'static LanguageInfo> {
        self.engines.iter().map(|e| e.info()).collect()
    }

    fn engine(&self, language: &str) -> Result<&dyn DiagramEngine, DiagramError> {
        self.engines
            .iter()
            .find(|e| e.id() == language)
            .map(|e| e.as_ref())
            .ok_or_else(|| DiagramError::UnknownLanguage(language.to_owned()))
    }

    /// Diagnostics for `source` in `language`.
    pub fn check(&self, language: &str, source: &str) -> Result<Vec<Diagnostic>, DiagramError> {
        Ok(self.engine(language)?.check(source))
    }

    /// Renders `source` to SVG text.
    pub fn render(
        &self,
        language: &str,
        source: &str,
        theme: Theme,
    ) -> Result<String, DiagramError> {
        self.engine(language)?.render(source, theme)
    }

    /// A stable hex key for caching the SVG of these inputs: a BLAKE3 hash
    /// of the language, its engine and version, this crate's output
    /// revision, the theme and the source, each length-prefixed.
    pub fn key(&self, language: &str, source: &str, theme: Theme) -> Result<String, DiagramError> {
        let engine = self.engine(language)?;
        let mut hasher = blake3::Hasher::new();
        for field in [
            "manang-diagram",
            OUTPUT_REVISION,
            engine.id(),
            engine.info().engine,
            theme.id(),
            source,
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        Ok(hasher.finalize().to_hex().to_string())
    }
}

fn args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(failed(DiagramError::InvalidArgument(
            "expected a JSON payload".to_owned(),
        ))),
    }
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, PluginError> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| {
        failed(DiagramError::InvalidArgument(format!(
            "`{key}` must be a string"
        )))
    })
}

fn theme(args: &Value) -> Result<Theme, PluginError> {
    match args.get("theme") {
        None | Some(Value::Null) => Ok(Theme::Dark),
        Some(Value::String(s)) => s.parse().map_err(failed),
        Some(_) => Err(failed(DiagramError::InvalidArgument(
            "`theme` must be a string".to_owned(),
        ))),
    }
}

fn failed(error: DiagramError) -> PluginError {
    PluginError::Failed(error.to_string())
}

impl Plugin for DiagramPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let a = args(input)?;
        match command {
            "diagram.languages" => Ok(Payload::Json(json!({ "languages": self.languages() }))),
            "diagram.check" => {
                let diagnostics = self
                    .check(string(&a, "language")?, string(&a, "source")?)
                    .map_err(failed)?;
                Ok(Payload::Json(json!({ "diagnostics": diagnostics })))
            }
            "diagram.render" => {
                let svg = self
                    .render(string(&a, "language")?, string(&a, "source")?, theme(&a)?)
                    .map_err(failed)?;
                Ok(Payload::Binary(Arc::from(svg.into_bytes())))
            }
            "diagram.layout" => {
                let language = a
                    .get("language")
                    .and_then(Value::as_str)
                    .unwrap_or("mermaid");
                let source = string(&a, "source")?;
                let laid_out = self
                    .engine(language)
                    .and_then(|e| e.layout(source))
                    .map_err(failed)?;
                Ok(Payload::Json(laid_out))
            }
            "diagram.key" => {
                let key = self
                    .key(string(&a, "language")?, string(&a, "source")?, theme(&a)?)
                    .map_err(failed)?;
                Ok(Payload::Json(json!({ "key": key })))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
