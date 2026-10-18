use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use manang_kernel::{
    Access, Capability, Context, EventBus, Hook, Manifest, Payload, Plugin, PluginError,
};
use parking_lot::{Mutex, RwLock};
use serde_json::{Value, json};

use crate::builtin::{Consent, EngineError, EnginePlan, Fetcher, HttpFetcher, Progress};
use crate::diagnostics::{Diagnostic, parse_bibtex_log, parse_tex_log};
use crate::driver::{
    BackendChoice, Build, CompileSpec, LatexConfig, compile_with, engine_source, engine_store,
    installed_builtin,
};
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

/// [`plugin`] that also publishes `latex.engine-progress` events while the
/// built-in engine downloads.
pub fn plugin_with_events(config: LatexConfig, events: EventBus) -> impl Plugin {
    LatexPlugin::new(config).with_events(events)
}

/// Topic of the events emitted while the built-in engine downloads.
pub const ENGINE_PROGRESS_TOPIC: &str = "latex.engine-progress";

/// Builds kept for `latex.export-pdf` and `latex.synctex`.
#[derive(Default)]
struct Builds {
    by_main: HashMap<String, Arc<Build>>,
    last: Option<Arc<Build>>,
}

/// Compiles LaTeX with the system TeX installation, or the optional
/// Tectonic worker when no TeX is installed.
pub struct LatexPlugin {
    /// Replaced by `latex.configure`; compiles take a snapshot.
    config: RwLock<LatexConfig>,
    builds: Mutex<Builds>,
    /// Detected once, on first use: the installed TeX does not change while
    /// the app runs (a restart picks up a new installation).
    tools: OnceLock<TexTools>,
    events: Option<EventBus>,
    fetcher: Arc<dyn Fetcher>,
    /// Set by `latex.engine-cancel`, cleared when an install starts.
    cancel: AtomicBool,
    /// One engine download at a time.
    installing: AtomicBool,
}

/// Clears the "install running" flag however the install ends.
struct InstallGuard<'a>(&'a AtomicBool);

impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl LatexPlugin {
    pub fn new(config: LatexConfig) -> Self {
        Self {
            config: RwLock::new(config),
            builds: Mutex::new(Builds::default()),
            tools: OnceLock::new(),
            events: None,
            fetcher: Arc::new(HttpFetcher),
            cancel: AtomicBool::new(false),
            installing: AtomicBool::new(false),
        }
    }

    /// Publish engine download progress on `events`.
    pub fn with_events(mut self, events: EventBus) -> Self {
        self.events = Some(events);
        self
    }

    /// Replace the HTTP client (tests use a local server).
    pub fn with_fetcher(mut self, fetcher: Arc<dyn Fetcher>) -> Self {
        self.fetcher = fetcher;
        self
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

    /// A snapshot of the current configuration.
    pub fn config(&self) -> LatexConfig {
        self.config.read().clone()
    }

    /// Apply `latex.configure {values: {id: value}}`. All-or-nothing: on a
    /// bad value nothing changes. Unknown ids are ignored so an older plugin
    /// tolerates settings a newer manifest added.
    fn configure(&self, args: &Value) -> Result<Value, LatexError> {
        let values = args
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| LatexError::BadArgument("values", "must be an object".to_owned()))?;
        let mut next = self.config();
        for (id, value) in values {
            match id.as_str() {
                SETTING_BACKEND => {
                    next.backend = value.as_str().and_then(parse_backend).ok_or_else(|| {
                        LatexError::BadArgument(
                            SETTING_BACKEND,
                            "must be auto, system or tectonic".to_owned(),
                        )
                    })?;
                }
                SETTING_ENGINE => {
                    next.engine = value.as_str().and_then(Engine::parse).ok_or_else(|| {
                        LatexError::BadArgument(
                            SETTING_ENGINE,
                            "must be pdflatex, xelatex or lualatex".to_owned(),
                        )
                    })?;
                }
                SETTING_LATEXMK => {
                    next.latexmk = value.as_bool().ok_or_else(|| {
                        LatexError::BadArgument(SETTING_LATEXMK, "must be a boolean".to_owned())
                    })?;
                }
                SETTING_FETCH => {
                    next.allow_network = value.as_bool().ok_or_else(|| {
                        LatexError::BadArgument(SETTING_FETCH, "must be a boolean".to_owned())
                    })?;
                }
                SETTING_TIMEOUT => {
                    let secs = value
                        .as_u64()
                        .filter(|s| (MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(s))
                        .ok_or_else(|| {
                            LatexError::BadArgument(
                                SETTING_TIMEOUT,
                                format!(
                                    "must be an integer from {MIN_TIMEOUT_SECS} to {MAX_TIMEOUT_SECS}"
                                ),
                            )
                        })?;
                    next.timeout = Duration::from_secs(secs);
                }
                _ => {}
            }
        }
        let out = settings_json(&next);
        *self.config.write() = next;
        Ok(out)
    }

    /// What can compile LaTeX on this machine, for `latex.status`.
    fn status_json(&self) -> Value {
        let config = self.config();
        let tools = self.tools();
        let engines: Vec<&str> = tools.engines.keys().map(|e| e.program()).collect();
        let tectonic = crate::driver::worker_available(&config);
        let tectonic_version =
            installed_builtin(&config).map(|(_, m)| format!("Tectonic {}", m.version));
        let default = match config.backend {
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
                    "version": tectonic_version,
                    "engines": ["xelatex"],
                    "latexmk": false,
                    "bibtex": tectonic,
                    "biber": false,
                }
            ],
            "default": default,
            "default_engine": config.engine.program(),
            "settings": settings_json(&config),
            "builtin": self.engine_status_json(),
        })
    }

    /// `latex.engine-status`: what the consented download would fetch and
    /// whether it is already installed.
    fn engine_status_json(&self) -> Value {
        let config = self.config();
        let installing = self.installing.load(Ordering::SeqCst);
        let store = engine_store(&config);
        let Some(source) = engine_source(&config) else {
            return json!({ "installed": false, "downloadable": false,
                "reason": "engine_manifest", "installing": installing });
        };
        let platform = source.platform.clone();
        let version = source.version().to_owned();
        let stored_in = store.as_ref().map(|s| s.version_dir(&version));
        match source.plan() {
            Ok(plan) => {
                let installed = store.as_ref().and_then(|s| s.installed(&plan));
                json!({
                    "installed": installed.is_some(),
                    "downloadable": store.is_some(),
                    "reason": Value::Null,
                    "version": plan.version,
                    "platform": plan.platform,
                    "size": plan.size,
                    "sha256": plan.sha256,
                    "url_host": plan.host,
                    "package_host": plan.package_host,
                    "stored_in": stored_in,
                    "installing": installing,
                })
            }
            Err(e) => json!({
                "installed": false,
                "downloadable": false,
                "reason": e.code(),
                "message": e.to_string(),
                "version": version,
                "platform": platform,
                "stored_in": stored_in,
                "installing": installing,
            }),
        }
    }

    fn emit_progress(&self, progress: Progress) {
        if let Some(events) = &self.events {
            events.emit(
                ENGINE_PROGRESS_TOPIC,
                Payload::Json(json!({
                    "phase": progress.phase,
                    "received": progress.received,
                    "total": progress.total,
                })),
            );
        }
    }

    /// `latex.engine-install {consent: true}`. Without an explicit `true`
    /// nothing happens: no request is made, no file is written.
    fn engine_install(&self, args: &Value) -> Result<Value, EngineError> {
        let granted = args.get("consent").and_then(Value::as_bool) == Some(true);
        let consent = Consent::from_flag(granted)?;
        let config = self.config();
        let source = engine_source(&config)
            .ok_or_else(|| EngineError::BadManifest("no manifest".to_owned()))?;
        let plan: EnginePlan = source.plan()?;
        let store = engine_store(&config).ok_or_else(|| {
            EngineError::BadManifest("no folder is set for the built-in engine".to_owned())
        })?;
        if self
            .installing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(EngineError::Busy);
        }
        let _guard = InstallGuard(&self.installing);
        self.cancel.store(false, Ordering::SeqCst);
        store.install(
            &plan,
            consent,
            self.fetcher.as_ref(),
            &self.cancel,
            &mut |p| {
                self.emit_progress(p);
            },
        )?;
        Ok(self.engine_status_json())
    }

    fn engine_remove(&self) -> Result<Value, EngineError> {
        if self.installing.load(Ordering::SeqCst) {
            return Err(EngineError::Busy);
        }
        if let Some(store) = engine_store(&self.config()) {
            store.remove()?;
        }
        Ok(self.engine_status_json())
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
        // A snapshot: a concurrent `latex.configure` affects the next compile.
        let config = self.config();
        let spec = parse_spec(args, &config)?;
        let main = spec.main.clone();
        let build = compile_with(&config, &spec, self.tools())?;
        Ok((main.clone(), self.remember(&main, build)))
    }
}

/// Setting ids declared in `manifest.toml`.
const SETTING_BACKEND: &str = "latex.backend";
const SETTING_ENGINE: &str = "latex.engine";
const SETTING_LATEXMK: &str = "latex.latexmk";
const SETTING_TIMEOUT: &str = "latex.timeout-seconds";
const SETTING_FETCH: &str = "latex.fetch-packages";
/// Bounds of `latex.timeout-seconds`; keep in step with `manifest.toml`.
const MIN_TIMEOUT_SECS: u64 = 5;
const MAX_TIMEOUT_SECS: u64 = 3600;

fn parse_backend(name: &str) -> Option<BackendChoice> {
    match name {
        "auto" => Some(BackendChoice::Auto),
        "system" => Some(BackendChoice::System),
        "tectonic" => Some(BackendChoice::Tectonic),
        _ => None,
    }
}

fn backend_name(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Auto => "auto",
        BackendChoice::System => "system",
        BackendChoice::Tectonic => "tectonic",
    }
}

/// The user-facing configuration, keyed by setting id.
fn settings_json(config: &LatexConfig) -> Value {
    json!({
        SETTING_BACKEND: backend_name(config.backend),
        SETTING_ENGINE: config.engine.program(),
        SETTING_LATEXMK: config.latexmk,
        SETTING_TIMEOUT: config.timeout.as_secs(),
        SETTING_FETCH: config.allow_network,
    })
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

fn engine_failed(error: &EngineError) -> PluginError {
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
            "latex.engine-status" => Ok(Payload::Json(self.engine_status_json())),
            "latex.engine-install" => self
                .engine_install(&args)
                .map(Payload::Json)
                .map_err(|e| engine_failed(&e)),
            "latex.engine-cancel" => {
                self.cancel.store(true, Ordering::SeqCst);
                Ok(Payload::Json(json!({ "ok": true })))
            }
            "latex.engine-remove" => self
                .engine_remove()
                .map(Payload::Json)
                .map_err(|e| engine_failed(&e)),
            "latex.configure" => self
                .configure(&args)
                .map(Payload::Json)
                .map_err(|e| failed(&e)),
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
