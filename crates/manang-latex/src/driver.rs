//! Runs one compile: materialise inputs, launch the worker under a hard
//! timeout, collect the PDF, log and SyncTeX data, and parse diagnostics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

use crate::builtin::{EngineSource, EngineStore, InstalledEngine};
use crate::diagnostics::{Diagnostic, Severity, parse_bibtex_log, parse_tex_log};
use crate::engine::{FailureKind, WorkerOutcome, WorkerRequest};
use crate::error::LatexError;
use crate::synctex::Synctex;
use crate::system::{self, Engine, SystemSpec, TexTools};

/// The build date embedded in PDFs unless overridden, so identical input
/// gives identical output. 2023-11-14T22:13:20Z.
const DEFAULT_BUILD_EPOCH: u64 = 1_700_000_000;
/// Upper bound on virtual source files handed to one compile.
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// Plugin-wide settings. The host sets the paths at construction; the user
/// settings (`backend`, `engine`, `latexmk`, `timeout`) are updated at
/// runtime through `latex.configure`.
#[derive(Debug, Clone)]
pub struct LatexConfig {
    /// The `manang-latex-worker` executable. `None` searches
    /// `$MANANG_LATEX_WORKER`, then next to the running executable.
    pub worker: Option<PathBuf>,
    /// The folder the consented, downloaded engine lives in
    /// (`<app data>/engines`). `None` means no built-in engine can be
    /// installed or found.
    pub engines_dir: Option<PathBuf>,
    /// Which engine release this build offers. `None` selects the manifest
    /// embedded in the app; tests inject their own.
    pub engine_source: Option<EngineSource>,
    /// Holds `bundles/` and `formats/`. `None` selects
    /// `$MANANG_LATEX_CACHE` or the per-user cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Bundle location; `None` selects Tectonic's default bundle.
    pub bundle: Option<String>,
    /// Let the built-in engine fetch TeX packages it has not cached yet (the
    /// `latex.fetch-packages` setting). On by default: the engine only exists
    /// because the user agreed to download it, and the consent text says packages
    /// are fetched on first use. A request can still override it.
    pub allow_network: bool,
    /// Hard wall-clock limit per compile.
    pub timeout: Duration,
    /// Which backend compiles. `Auto` prefers the system TeX installation.
    pub backend: BackendChoice,
    /// Engine used when a request does not name one.
    pub engine: Engine,
    /// Use `latexmk` with the system backend when it is installed.
    pub latexmk: bool,
}

/// Which backend compiles LaTeX (ADR 0004, amendment of 2026-10-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendChoice {
    /// The system TeX installation when one is found, otherwise the optional
    /// Tectonic worker when it is installed.
    #[default]
    Auto,
    System,
    Tectonic,
}

impl Default for LatexConfig {
    fn default() -> Self {
        Self {
            worker: None,
            engines_dir: None,
            engine_source: None,
            cache_dir: None,
            bundle: None,
            allow_network: true,
            timeout: Duration::from_secs(60),
            backend: BackendChoice::Auto,
            engine: Engine::Pdflatex,
            latexmk: true,
        }
    }
}

/// One compile request, after argument parsing.
#[derive(Debug, Clone)]
pub struct CompileSpec {
    /// Path of the main file: on disk, or a key of `sources`.
    pub main: String,
    /// A self-contained virtual project (relative path -> text).
    pub sources: Option<BTreeMap<String, String>>,
    pub allow_network: bool,
    pub timeout: Duration,
    pub synctex: bool,
    pub build_epoch: u64,
    /// Engine for the system backend; `None` uses the configured default.
    pub engine: Option<Engine>,
    /// Override the configured `latexmk` preference.
    pub latexmk: Option<bool>,
}

impl CompileSpec {
    pub fn new(main: impl Into<String>, config: &LatexConfig) -> Self {
        Self {
            main: main.into(),
            sources: None,
            allow_network: config.allow_network,
            timeout: config.timeout,
            synctex: true,
            build_epoch: std::env::var("SOURCE_DATE_EPOCH")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_BUILD_EPOCH),
            engine: None,
            latexmk: None,
        }
    }
}

/// The result of a compile that reached TeX (errors in the document are
/// diagnostics, not `Err`).
#[derive(Debug, Clone)]
pub struct Build {
    pub ok: bool,
    pub pdf: Option<Arc<[u8]>>,
    pub synctex: Option<Arc<Synctex>>,
    /// Directory the build resolved relative paths against.
    pub root: String,
    pub diagnostics: Vec<Diagnostic>,
    pub pages: Option<u32>,
    pub duration: Duration,
    /// `"system"` or `"tectonic"`.
    pub backend: &'static str,
    pub engine: Engine,
    /// The TeX distribution's version line, when known.
    pub tex_version: Option<String>,
}

fn io_err(path: &Path, source: std::io::Error) -> LatexError {
    LatexError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn find_worker(config: &LatexConfig) -> Result<PathBuf, LatexError> {
    let name = format!("manang-latex-worker{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = &config.worker {
        return Ok(path.clone());
    }
    if let Some(path) = std::env::var_os("MANANG_LATEX_WORKER") {
        return Ok(PathBuf::from(path));
    }
    if let Some((path, _)) = installed_builtin(config) {
        return Ok(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        // Next to the app, or one level up (test binaries live in `deps/`).
        for dir in exe.ancestors().skip(1).take(2) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(LatexError::WorkerMissing(name))
}

/// The release this build offers: injected for tests, else the embedded one.
pub fn engine_source(config: &LatexConfig) -> Option<EngineSource> {
    config
        .engine_source
        .clone()
        .or_else(|| EngineSource::embedded().ok())
}

/// The store the consented download installs into.
pub fn engine_store(config: &LatexConfig) -> Option<EngineStore> {
    config.engines_dir.clone().map(EngineStore::new)
}

/// The downloaded engine's worker, when a complete verified install exists.
pub fn installed_builtin(config: &LatexConfig) -> Option<(PathBuf, InstalledEngine)> {
    engine_store(config)?.installed_worker(&engine_source(config)?)
}

/// Whether the optional Tectonic worker is installed.
pub fn worker_available(config: &LatexConfig) -> bool {
    find_worker(config).is_ok_and(|path| path.is_file())
}

/// Where bundles and formats live.
pub fn resolve_cache_dir(config: &LatexConfig) -> Result<PathBuf, LatexError> {
    if let Some(dir) = &config.cache_dir {
        return Ok(dir.clone());
    }
    if let Some(dir) = std::env::var_os("MANANG_LATEX_CACHE") {
        return Ok(PathBuf::from(dir));
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or_else(|| {
            LatexError::BadArgument(
                "cache_dir",
                "no cache directory could be determined".to_owned(),
            )
        })?;
    Ok(base.join("manang").join("latex"))
}

/// A virtual-project path must be relative and stay inside the project.
fn safe_relative(path: &str) -> Result<PathBuf, LatexError> {
    let p = Path::new(path);
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(LatexError::UnsafePath(path.to_owned())),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(LatexError::UnsafePath(path.to_owned()));
    }
    Ok(out)
}

struct Inputs {
    main: PathBuf,
    root: PathBuf,
    /// Name diagnostics use for the main file.
    main_name: String,
}

fn prepare_inputs(spec: &CompileSpec, src_dir: &Path) -> Result<Inputs, LatexError> {
    match &spec.sources {
        Some(sources) => {
            let mut total = 0usize;
            for (path, text) in sources {
                total = total.saturating_add(text.len());
                if total > MAX_SOURCE_BYTES {
                    return Err(LatexError::BadArgument(
                        "sources",
                        format!("more than {MAX_SOURCE_BYTES} bytes"),
                    ));
                }
                let target = src_dir.join(safe_relative(path)?);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
                }
                fs::write(&target, text).map_err(|e| io_err(&target, e))?;
            }
            let main_rel = safe_relative(&spec.main)?;
            let main = src_dir.join(&main_rel);
            if !sources.contains_key(&spec.main) && !main.is_file() {
                return Err(LatexError::BadArgument(
                    "main",
                    format!("`{}` is not one of the provided sources", spec.main),
                ));
            }
            Ok(Inputs {
                main,
                root: src_dir.to_path_buf(),
                main_name: main_rel.to_string_lossy().into_owned(),
            })
        }
        None => {
            let main =
                fs::canonicalize(&spec.main).map_err(|e| io_err(Path::new(&spec.main), e))?;
            let root = main.parent().map(Path::to_path_buf).ok_or_else(|| {
                LatexError::BadArgument("main", "has no parent directory".to_owned())
            })?;
            let main_name = main
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| spec.main.clone());
            Ok(Inputs {
                main,
                root,
                main_name,
            })
        }
    }
}

fn read_optional(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

/// `Output written on main.xdv (3 pages, 12345 bytes).`
///
/// TeX hard-wraps its log at 79 columns, and the build folder's path is long
/// enough to split this very line, so the log is un-wrapped first.
fn page_count(log: &str) -> Option<u32> {
    let lines = crate::diagnostics::unwrap_lines(log);
    let line = lines
        .iter()
        .rev()
        .find(|l| l.starts_with("Output written on"))?;
    let after = line.split('(').nth(1)?;
    after.split_whitespace().next()?.parse().ok()
}

/// Compile with the tools found on `PATH` right now. The plugin caches the
/// detection and calls [`compile_with`] instead.
pub fn compile(config: &LatexConfig, spec: &CompileSpec) -> Result<Build, LatexError> {
    compile_with(config, spec, &TexTools::detect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    System,
    Tectonic,
}

fn choose_backend(config: &LatexConfig, tools: &TexTools) -> Result<Backend, LatexError> {
    match config.backend {
        BackendChoice::System if tools.is_available() => Ok(Backend::System),
        BackendChoice::System => Err(LatexError::NoTexInstallation),
        BackendChoice::Tectonic => Ok(Backend::Tectonic),
        BackendChoice::Auto if tools.is_available() => Ok(Backend::System),
        BackendChoice::Auto if worker_available(config) => Ok(Backend::Tectonic),
        BackendChoice::Auto => Err(LatexError::NoTexInstallation),
    }
}

/// FNV-1a: a stable, dependency-free key for per-project build folders.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// A persistent build folder per project and engine, so warm builds reuse
/// aux files (latexmk then only reruns what changed).
fn build_dir_for(config: &LatexConfig, root: &Path, engine: Engine) -> Result<PathBuf, LatexError> {
    let key = fnv1a(root.as_os_str().as_encoded_bytes());
    Ok(resolve_cache_dir(config)?
        .join("builds")
        .join(format!("{key:016x}-{}", engine.program())))
}

/// What a backend run left behind.
struct RunOutcome {
    stem: String,
    /// The document failed to compile (TeX errors), with a short reason.
    failed: Option<String>,
}

/// The last lines the TeX tools printed, for a failure with nothing in the log.
fn tool_output_tail(out_dir: &Path) -> String {
    let text = read_optional(&out_dir.join(system::TOOL_OUTPUT))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    // latexmk ends with generic advice; start at the first line that says what
    // went wrong when there is one, else take the last few lines.
    let start = lines
        .iter()
        .position(|l| {
            let lower = l.to_ascii_lowercase();
            lower.contains("error") || lower.contains("fatal") || lower.starts_with('!')
        })
        .unwrap_or_else(|| lines.len().saturating_sub(6));
    let tail = &lines[start..lines.len().min(start + 6)];
    if tail.is_empty() {
        String::new()
    } else {
        format!(": {}", tail.join(" | "))
    }
}

/// Compile in a private directory. Blocks until TeX exits or the timeout
/// kills it; call from a CPU-bound thread (Rayon), never from Tokio.
pub fn compile_with(
    config: &LatexConfig,
    spec: &CompileSpec,
    tools: &TexTools,
) -> Result<Build, LatexError> {
    let started = Instant::now();
    let backend = choose_backend(config, tools)?;

    let scratch = tempfile::Builder::new()
        .prefix("manang-latex-")
        .tempdir()
        .map_err(|e| io_err(&std::env::temp_dir(), e))?;
    let src_dir = scratch.path().join("src");
    fs::create_dir_all(&src_dir).map_err(|e| io_err(&src_dir, e))?;
    let inputs = prepare_inputs(spec, &src_dir)?;

    let (out_dir, outcome, engine, tex_version, backend_name) = match backend {
        Backend::System => {
            let engine = spec.engine.unwrap_or(config.engine);
            let out_dir = if spec.sources.is_some() {
                scratch.path().join("out")
            } else {
                build_dir_for(config, &inputs.root, engine)?
            };
            let run = system::run(
                tools,
                &inputs.main,
                &inputs.root,
                &out_dir,
                &SystemSpec {
                    engine,
                    latexmk: spec.latexmk.unwrap_or(config.latexmk),
                    synctex: spec.synctex,
                    timeout: spec.timeout,
                    build_epoch: spec.build_epoch,
                },
            )?;
            let failed = run.failure.map(|status| {
                let detail = tool_output_tail(&out_dir);
                format!(
                    "{} stopped ({status}); see the log{detail}",
                    engine.program()
                )
            });
            let outcome = RunOutcome {
                stem: run.stem,
                failed,
            };
            (out_dir, outcome, engine, tools.version.clone(), "system")
        }
        Backend::Tectonic => {
            let out_dir = scratch.path().join("out");
            fs::create_dir_all(&out_dir).map_err(|e| io_err(&out_dir, e))?;
            let outcome = run_tectonic(config, spec, &inputs, scratch.path(), &out_dir)?;
            // Name the engine that ran: "Tectonic 0.17.0" for the downloaded
            // one, nothing when a developer pointed at their own worker.
            let version = installed_builtin(config)
                .filter(|(path, _)| find_worker(config).is_ok_and(|w| &w == path))
                .map(|(_, m)| format!("Tectonic {}", m.version));
            (out_dir, outcome, Engine::Xelatex, version, "tectonic")
        }
    };

    let stem = &outcome.stem;
    let log = read_optional(&out_dir.join(format!("{stem}.log")))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let blg = read_optional(&out_dir.join(format!("{stem}.blg")))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    let mut diagnostics = parse_tex_log(&log, &inputs.main_name);
    diagnostics.extend(parse_bibtex_log(&blg, &format!("{stem}.aux")));
    if let Some(reason) = &outcome.failed
        && !diagnostics.iter().any(|d| d.severity == Severity::Error)
    {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: reason.clone(),
            file: Some(inputs.main_name.clone()),
            line: None,
            hint: None,
        });
    }

    let pdf = if outcome.failed.is_none() {
        read_optional(&out_dir.join(format!("{stem}.pdf"))).map(Arc::<[u8]>::from)
    } else {
        None
    };
    let synctex = if pdf.is_some() && spec.synctex {
        read_optional(&out_dir.join(format!("{stem}.synctex.gz")))
            .and_then(|bytes| Synctex::from_gz(&bytes).ok())
            .map(Arc::new)
    } else {
        None
    };

    Ok(Build {
        ok: pdf.is_some(),
        pages: page_count(&log),
        pdf,
        synctex,
        root: inputs.root.to_string_lossy().into_owned(),
        diagnostics,
        duration: started.elapsed(),
        backend: backend_name,
        engine,
        tex_version,
    })
}

/// The optional Tectonic engine, in its separate worker process.
fn run_tectonic(
    config: &LatexConfig,
    spec: &CompileSpec,
    inputs: &Inputs,
    scratch: &Path,
    out_dir: &Path,
) -> Result<RunOutcome, LatexError> {
    let worker = find_worker(config)?;
    let cache_dir = resolve_cache_dir(config)?;

    let request = WorkerRequest {
        main: inputs.main.clone(),
        root: inputs.root.clone(),
        out_dir: out_dir.to_path_buf(),
        bundle: config.bundle.clone(),
        cache_dir,
        only_cached: !spec.allow_network,
        synctex: spec.synctex,
        build_epoch: spec.build_epoch,
    };
    let request_path = scratch.join("request.json");
    let json = serde_json::to_vec(&request)
        .map_err(|e| LatexError::Engine(format!("could not encode the request: {e}")))?;
    fs::write(&request_path, json).map_err(|e| io_err(&request_path, e))?;

    let stderr_path = scratch.join("worker.stderr");
    let stderr = fs::File::create(&stderr_path).map_err(|e| io_err(&stderr_path, e))?;
    let mut command = Command::new(&worker);
    if !spec.allow_network {
        // Defence in depth: even if the engine tried to fetch something, the
        // only route out is a closed local port, so it fails at once.
        for var in ["https_proxy", "http_proxy", "all_proxy"] {
            command.env(var, "http://127.0.0.1:9");
            command.env(var.to_uppercase(), "http://127.0.0.1:9");
        }
        command.env_remove("no_proxy").env_remove("NO_PROXY");
    }
    let mut child = command
        .arg(&request_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LatexError::WorkerMissing(worker.display().to_string())
            } else {
                LatexError::WorkerSpawn(e)
            }
        })?;

    let status = match child
        .wait_timeout(spec.timeout)
        .map_err(LatexError::WorkerSpawn)?
    {
        Some(status) => status,
        None => {
            // Best effort: the process may have exited between the two calls.
            let _ = child.kill();
            let _ = child.wait();
            return Err(LatexError::Timeout(spec.timeout));
        }
    };
    if !status.success() {
        let detail = fs::read_to_string(&stderr_path).unwrap_or_default();
        return Err(LatexError::WorkerCrashed(format!(
            "{status}: {}",
            detail.trim()
        )));
    }

    let outcome: WorkerOutcome = fs::read(out_dir.join("outcome.json"))
        .map_err(|e| LatexError::WorkerCrashed(format!("no outcome written: {e}")))
        .and_then(|b| {
            serde_json::from_slice(&b)
                .map_err(|e| LatexError::WorkerCrashed(format!("unreadable outcome: {e}")))
        })?;

    match outcome.failure {
        Some(FailureKind::Bundle) => Err(LatexError::BundleUnavailable(outcome.message)),
        Some(FailureKind::Engine) => Err(LatexError::Engine(outcome.message)),
        Some(FailureKind::Tex) => Ok(RunOutcome {
            stem: outcome.stem,
            failed: Some(outcome.message),
        }),
        None => Ok(RunOutcome {
            stem: outcome.stem,
            failed: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_failure_without_log_errors_shows_what_the_tools_printed() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(tool_output_tail(dir.path()), "");
        fs::write(
            dir.path().join(system::TOOL_OUTPUT),
            "Running 'pdflatex -halt-on-error main.tex'\n\nThis is pdfTeX\n! Emergency stop.\nLatexmk: advice\n",
        )
        .expect("write");
        let tail = tool_output_tail(dir.path());
        assert!(tail.starts_with(": Running 'pdflatex"), "{tail}");
        assert!(tail.contains("Emergency stop"), "{tail}");
    }

    use super::*;

    #[test]
    fn safe_relative_rejects_escapes() {
        assert!(safe_relative("a/b.tex").is_ok());
        assert!(safe_relative("./a.tex").is_ok());
        for bad in ["", "..", "../x", "a/../../x", "/etc/passwd", "."] {
            assert!(
                matches!(safe_relative(bad), Err(LatexError::UnsafePath(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn auto_prefers_system_tex_and_reports_when_nothing_is_installed() {
        let mut tools = TexTools::default();
        let config = LatexConfig {
            worker: Some(PathBuf::from("/nonexistent/manang-latex-worker")),
            ..LatexConfig::default()
        };
        assert!(matches!(
            choose_backend(&config, &tools),
            Err(LatexError::NoTexInstallation)
        ));
        tools
            .engines
            .insert(Engine::Pdflatex, PathBuf::from("/usr/bin/pdflatex"));
        assert_eq!(choose_backend(&config, &tools).ok(), Some(Backend::System));
        let forced = LatexConfig {
            backend: BackendChoice::Tectonic,
            ..config
        };
        assert_eq!(
            choose_backend(&forced, &tools).ok(),
            Some(Backend::Tectonic)
        );
    }

    #[test]
    fn build_dirs_are_stable_per_project_and_engine() {
        let config = LatexConfig {
            cache_dir: Some(PathBuf::from("/cache")),
            ..LatexConfig::default()
        };
        let a = build_dir_for(&config, Path::new("/p/one"), Engine::Pdflatex).expect("dir");
        let b = build_dir_for(&config, Path::new("/p/one"), Engine::Pdflatex).expect("dir");
        let c = build_dir_for(&config, Path::new("/p/one"), Engine::Xelatex).expect("dir");
        let d = build_dir_for(&config, Path::new("/p/two"), Engine::Pdflatex).expect("dir");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert!(a.starts_with("/cache/builds"));
    }

    #[test]
    fn counts_pages_from_the_log() {
        assert_eq!(
            page_count("x\nOutput written on main.xdv (3 pages, 9 bytes).\n"),
            Some(3)
        );
        assert_eq!(
            page_count("Output written on main.xdv (1 page, 9 bytes)."),
            Some(1)
        );
        assert_eq!(page_count("nothing"), None);
    }

    #[test]
    fn counts_pages_when_tex_wrapped_the_line() {
        // TeX breaks the log at 79 columns; a long build path splits the line
        // right inside "(1 page".
        let line = "Output written on /home/sapiens/.cache/work.fundaments.manang/manang/latex/builds/0123456789abcdef/main.pdf (1 page, 70193 bytes).";
        let wrapped = format!("{}\n{}\n", &line[..79], &line[79..]);
        assert!(
            wrapped
                .lines()
                .next()
                .is_some_and(|l| l.chars().count() == 79)
        );
        assert_eq!(page_count(&wrapped), Some(1));
        let many = line.replace("(1 page", "(12 pages");
        let wrapped = format!("{}\n{}\n", &many[..79], &many[79..]);
        assert_eq!(page_count(&wrapped), Some(12));
    }

    #[test]
    fn missing_worker_is_a_clear_error() {
        let config = LatexConfig {
            worker: Some(PathBuf::from("/nonexistent/manang-latex-worker")),
            cache_dir: Some(std::env::temp_dir()),
            backend: BackendChoice::Tectonic,
            ..LatexConfig::default()
        };
        let mut sources = BTreeMap::new();
        sources.insert("main.tex".to_owned(), "x".to_owned());
        let mut spec = CompileSpec::new("main.tex", &config);
        spec.sources = Some(sources);
        assert!(matches!(
            compile(&config, &spec),
            Err(LatexError::WorkerMissing(_))
        ));
    }
}
