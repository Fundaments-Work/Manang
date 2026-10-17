//! Runs one compile: materialise inputs, launch the worker under a hard
//! timeout, collect the PDF, log and SyncTeX data, and parse diagnostics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

use crate::diagnostics::{Diagnostic, Severity, parse_bibtex_log, parse_tex_log};
use crate::engine::{FailureKind, WorkerOutcome, WorkerRequest};
use crate::error::LatexError;
use crate::synctex::Synctex;

/// The build date embedded in PDFs unless overridden, so identical input
/// gives identical output. 2023-11-14T22:13:20Z.
const DEFAULT_BUILD_EPOCH: u64 = 1_700_000_000;
/// Upper bound on virtual source files handed to one compile.
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// Plugin-wide settings, fixed when the plugin is constructed.
#[derive(Debug, Clone)]
pub struct LatexConfig {
    /// The `manang-latex-worker` executable. `None` searches
    /// `$MANANG_LATEX_WORKER`, then next to the running executable.
    pub worker: Option<PathBuf>,
    /// Holds `bundles/` and `formats/`. `None` selects
    /// `$MANANG_LATEX_CACHE` or the per-user cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Bundle location; `None` selects Tectonic's default bundle.
    pub bundle: Option<String>,
    /// Allow fetching missing bundle files by default. Off unless asked.
    pub allow_network: bool,
    /// Hard wall-clock limit per compile.
    pub timeout: Duration,
}

impl Default for LatexConfig {
    fn default() -> Self {
        Self {
            worker: None,
            cache_dir: None,
            bundle: None,
            allow_network: false,
            timeout: Duration::from_secs(60),
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
fn page_count(log: &str) -> Option<u32> {
    let line = log
        .lines()
        .rev()
        .find(|l| l.starts_with("Output written on"))?;
    let after = line.split('(').nth(1)?;
    after.split_whitespace().next()?.parse().ok()
}

/// Compile in a private temp directory. Blocks until the worker exits or the
/// timeout kills it; call from a CPU-bound thread (Rayon), never from Tokio.
pub fn compile(config: &LatexConfig, spec: &CompileSpec) -> Result<Build, LatexError> {
    let started = Instant::now();
    let worker = find_worker(config)?;
    let cache_dir = resolve_cache_dir(config)?;

    let scratch = tempfile::Builder::new()
        .prefix("manang-latex-")
        .tempdir()
        .map_err(|e| io_err(&std::env::temp_dir(), e))?;
    let src_dir = scratch.path().join("src");
    let out_dir = scratch.path().join("out");
    for dir in [&src_dir, &out_dir] {
        fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    }
    let inputs = prepare_inputs(spec, &src_dir)?;

    let request = WorkerRequest {
        main: inputs.main.clone(),
        root: inputs.root.clone(),
        out_dir: out_dir.clone(),
        bundle: config.bundle.clone(),
        cache_dir,
        only_cached: !spec.allow_network,
        synctex: spec.synctex,
        build_epoch: spec.build_epoch,
    };
    let request_path = scratch.path().join("request.json");
    let json = serde_json::to_vec(&request)
        .map_err(|e| LatexError::Engine(format!("could not encode the request: {e}")))?;
    fs::write(&request_path, json).map_err(|e| io_err(&request_path, e))?;

    let stderr_path = scratch.path().join("worker.stderr");
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
        Some(FailureKind::Bundle) => {
            return Err(LatexError::BundleUnavailable(outcome.message));
        }
        Some(FailureKind::Engine) => return Err(LatexError::Engine(outcome.message)),
        Some(FailureKind::Tex) | None => {}
    }

    let stem = &outcome.stem;
    let log = read_optional(&out_dir.join(format!("{stem}.log")))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let blg = read_optional(&out_dir.join(format!("{stem}.blg")))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    let mut diagnostics = parse_tex_log(&log, &inputs.main_name);
    diagnostics.extend(parse_bibtex_log(&blg, &format!("{stem}.aux")));
    if outcome.failure.is_some() && !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: outcome.message.clone(),
            file: Some(inputs.main_name.clone()),
            line: None,
            hint: None,
        });
    }

    let pdf = if outcome.failure.is_none() {
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
    })
}

#[cfg(test)]
mod tests {
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
    fn missing_worker_is_a_clear_error() {
        let config = LatexConfig {
            worker: Some(PathBuf::from("/nonexistent/manang-latex-worker")),
            cache_dir: Some(std::env::temp_dir()),
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
