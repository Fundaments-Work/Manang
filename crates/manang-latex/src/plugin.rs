use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use manang_kernel::{Access, Capability, Context, Hook, Manifest, Payload, Plugin, PluginError};
use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::diagnostics::{Diagnostic, parse_bibtex_log, parse_tex_log};
use crate::driver::{BackendChoice, Build, CompileSpec, LatexConfig, compile_with};
use crate::error::LatexError;
use crate::system::{Engine, TexTools};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The LaTeX plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the plugin. Commands block until the compile finishes, so the
/// caller must run them on a CPU-bound (Rayon) thread, never a Tokio worker.
pub fn plugin(config: LatexConfig) -> impl Plugin {
    LatexPlugin::new(config)
}

/// Builds kept for `latex.export-pdf` and `latex.synctex`.
#[derive(Default)]
struct Builds {
    by_main: HashMap<String, Arc<Build>>,
    last: Option<Arc<Build>>,
}

/// Compiles LaTeX with the system TeX installation, or the optional
/// Tectonic worker when no TeX is installed.
pub struct LatexPlugin {
    config: LatexConfig,
    builds: Mutex<Builds>,
    /// Detected once, on first use: the installed TeX does not change while
    /// the app runs (a restart picks up a new installation).
    tools: OnceLock<TexTools>,
}

impl LatexPlugin {
    pub fn new(config: LatexConfig) -> Self {
        Self {
            config,
            builds: Mutex::new(Builds::default()),
            tools: OnceLock::new(),
        }
    }

    /// Use already-detected tools (tests inject a fixed set).
    pub fn with_tools(config: LatexConfig, tools: TexTools) -> Self {
        let plugin = Self::new(config);
        let _ = plugin.tools.set(tools);
        plugin
    }

    fn tools(&self) -> &TexTools {
        self.tools.get_or_init(TexTools::detect)
    }

    /// What can compile LaTeX on this machine, for `latex.status`.
    fn status_json(&self) -> Value {
        let tools = self.tools();
        let engines: Vec<&str> = tools.engines.keys().map(|e| e.program()).collect();
        let tectonic = crate::driver::worker_available(&self.config);
        let default = match self.config.backend {
            BackendChoice::System => "system",
            BackendChoice::Tectonic => "tectonic",
            BackendChoice::Auto if tools.is_available() => "system",
            BackendChoice::Auto if tectonic => "tectonic",
            BackendChoice::Auto => "none",
        };
        json!({
            "backends": [
                {
                    "id": "system",
                    "available": tools.is_available(),
                    "version": tools.version,
                    "engines": engines,
                    "latexmk": tools.latexmk.is_some(),
                    "bibtex": tools.bibtex.is_some(),
                    "biber": tools.biber.is_some(),
                },
                {
                    "id": "tectonic",
                    "available": tectonic,
                    "version": Value::Null,
                    "engines": ["xelatex"],
                    "latexmk": false,
                    "bibtex": tectonic,
                    "biber": false,
                }
            ],
            "default": default,
            "default_engine": self.config.engine.program(),
        })
    }

    fn remember(&self, main: &str, build: Build) -> Arc<Build> {
        let build = Arc::new(build);
        let mut builds = self.builds.lock();
        builds.by_main.insert(main.to_owned(), Arc::clone(&build));
        builds.last = Some(Arc::clone(&build));
        build
    }

    fn recall(&self, main: Option<&str>) -> Result<Arc<Build>, LatexError> {
        let builds = self.builds.lock();
        let found = match main {
            Some(main) => builds.by_main.get(main).cloned(),
            None => builds.last.clone(),
        };
        found.ok_or(LatexError::NoBuild)
    }

    fn run(&self, args: &Value) -> Result<(String, Arc<Build>), LatexError> {
        let spec = parse_spec(args, &self.config)?;
        let main = spec.main.clone();
        let build = compile_with(&self.config, &spec, self.tools())?;
        Ok((main.clone(), self.remember(&main, build)))
    }
}

fn parse_spec(args: &Value, config: &LatexConfig) -> Result<CompileSpec, LatexError> {
    let main = args
        .get("main")
        .and_then(Value::as_str)
        .ok_or_else(|| LatexError::BadArgument("main", "must be a string".to_owned()))?;
    let mut spec = CompileSpec::new(main, config);
    if let Some(sources) = args.get("sources").filter(|v| !v.is_null()) {
        let map = sources.as_object().ok_or_else(|| {
            LatexError::BadArgument("sources", "must be an object of path -> text".to_owned())
        })?;
        let mut out = BTreeMap::new();
        for (path, text) in map {
            let text = text.as_str().ok_or_else(|| {
                LatexError::BadArgument("sources", format!("`{path}` must be a string"))
            })?;
            out.insert(path.clone(), text.to_owned());
        }
        spec.sources = Some(out);
    }
    if let Some(v) = args.get("allow_network") {
        spec.allow_network = v.as_bool().ok_or_else(|| {
            LatexError::BadArgument("allow_network", "must be a boolean".to_owned())
        })?;
    }
    if let Some(v) = args.get("synctex") {
        spec.synctex = v
            .as_bool()
            .ok_or_else(|| LatexError::BadArgument("synctex", "must be a boolean".to_owned()))?;
    }
    if let Some(v) = args.get("timeout_ms") {
        let ms = v.as_u64().filter(|ms| *ms > 0).ok_or_else(|| {
            LatexError::BadArgument("timeout_ms", "must be a positive integer".to_owned())
        })?;
        spec.timeout = Duration::from_millis(ms);
    }
    if let Some(v) = args.get("engine").filter(|v| !v.is_null()) {
        let name = v
            .as_str()
            .ok_or_else(|| LatexError::BadArgument("engine", "must be a string".to_owned()))?;
        spec.engine = Some(Engine::parse(name).ok_or_else(|| {
            LatexError::BadArgument("engine", "must be pdflatex, xelatex or lualatex".to_owned())
        })?);
    }
    if let Some(v) = args.get("latexmk").filter(|v| !v.is_null()) {
        spec.latexmk =
            Some(v.as_bool().ok_or_else(|| {
                LatexError::BadArgument("latexmk", "must be a boolean".to_owned())
            })?);
    }
    if let Some(v) = args.get("build_epoch") {
        spec.build_epoch = v.as_u64().ok_or_else(|| {
            LatexError::BadArgument("build_epoch", "must be an unsigned integer".to_owned())
        })?;
    }
    Ok(spec)
}

fn json_args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(PluginError::Failed("expected a JSON payload".to_owned())),
    }
}

/// `[code] message`: the code is stable, the message is for people.
fn failed(error: &LatexError) -> PluginError {
    PluginError::Failed(format!("[{}] {error}", error.code()))
}

fn compile_json(build: &Build) -> Value {
    json!({
        "ok": build.ok,
        "pages": build.pages,
        "diagnostics": build.diagnostics,
        "duration_ms": u64::try_from(build.duration.as_millis()).unwrap_or(u64::MAX),
        "synctex": build.synctex.is_some(),
        "backend": build.backend,
        "engine": build.engine.program(),
        "tex_version": build.tex_version,
    })
}

fn number(args: &Value, key: &'static str) -> Result<f64, LatexError> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| LatexError::BadArgument(key, "must be a number".to_owned()))
}

fn synctex_lookup(build: &Build, args: &Value) -> Result<Value, LatexError> {
    let index = build.synctex.as_ref().ok_or(LatexError::NoSynctex)?;
    if let Some(file) = args.get("file").and_then(Value::as_str) {
        let line = number(args, "line")?;
        let line = u32::try_from(line as i64).map_err(|_| {
            LatexError::BadArgument("line", "must be a positive integer".to_owned())
        })?;
        let positions = index.forward(file, line, &build.root)?;
        Ok(json!({ "direction": "forward", "positions": positions }))
    } else if args.get("page").is_some() {
        let page = u32::try_from(number(args, "page")? as i64).map_err(|_| {
            LatexError::BadArgument("page", "must be a positive integer".to_owned())
        })?;
        let at = index.reverse(
            page,
            number(args, "x")? as f32,
            number(args, "y")? as f32,
            &build.root,
        )?;
        Ok(json!({ "direction": "reverse", "file": at.file, "line": at.line }))
    } else {
        Err(LatexError::BadArgument(
            "file|page",
            "give `file` and `line` (forward) or `page`, `x` and `y` (reverse)".to_owned(),
        ))
    }
}

/// `on_lint` input: `{log, bibtex_log?, main?}` parses text as is;
/// `{main, sources?}` compiles; anything else reports the last build.
fn lint(plugin: &LatexPlugin, input: &Value) -> Vec<Diagnostic> {
    if let Some(log) = input.get("log").and_then(Value::as_str) {
        let main = input
            .get("main")
            .and_then(Value::as_str)
            .unwrap_or("main.tex");
        let mut out = parse_tex_log(log, main);
        if let Some(blg) = input.get("bibtex_log").and_then(Value::as_str) {
            out.extend(parse_bibtex_log(blg, "main.aux"));
        }
        return out;
    }
    if input.get("main").is_some() {
        return match plugin.run(input) {
            Ok((_, build)) => build.diagnostics.clone(),
            // A linter must not fail the whole fan-out; surface the problem.
            Err(e) => vec![Diagnostic {
                severity: crate::diagnostics::Severity::Error,
                message: e.to_string(),
                file: None,
                line: None,
                hint: None,
            }],
        };
    }
    plugin
        .recall(None)
        .map(|b| b.diagnostics.clone())
        .unwrap_or_default()
}

impl Plugin for LatexPlugin {
    fn invoke(
        &self,
        ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let args = json_args(input)?;
        let read_project = Capability::Filesystem {
            access: Access::Read,
            scope: "project".to_owned(),
        };
        match command {
            "latex.status" => Ok(Payload::Json(self.status_json())),
            "latex.compile" => {
                ctx.require(&read_project)?;
                let (_, build) = self.run(&args).map_err(|e| failed(&e))?;
                Ok(Payload::Json(compile_json(&build)))
            }
            "latex.export-pdf" => {
                ctx.require(&read_project)?;
                let main = args.get("main").and_then(Value::as_str);
                // Reuse the latest build of `main` unless the caller asks
                // for a fresh compile or none exists yet.
                let reuse = !args
                    .get("recompile")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let build = match main.map(|m| self.recall(Some(m))) {
                    Some(Ok(build)) if reuse => build,
                    None if reuse => self.recall(None).map_err(|e| failed(&e))?,
                    _ => self.run(&args).map_err(|e| failed(&e))?.1,
                };
                match &build.pdf {
                    Some(pdf) => Ok(Payload::Binary(Arc::clone(pdf))),
                    None => Err(PluginError::Failed(format!(
                        "[compile_failed] the last compile produced no PDF: {}",
                        build
                            .diagnostics
                            .iter()
                            .find(|d| d.severity == crate::diagnostics::Severity::Error)
                            .map_or("see diagnostics", |d| d.message.as_str())
                    ))),
                }
            }
            "latex.synctex" => {
                let main = args.get("main").and_then(Value::as_str);
                let build = self.recall(main).map_err(|e| failed(&e))?;
                synctex_lookup(&build, &args)
                    .map(Payload::Json)
                    .map_err(|e| failed(&e))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }

    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        if hook != Hook::OnLint {
            return Ok(input);
        }
        let args = match &input {
            Payload::Json(v) => v.clone(),
            _ => Value::Null,
        };
        Ok(Payload::Json(json!({ "diagnostics": lint(self, &args) })))
    }
}
