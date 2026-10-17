//! The system TeX backend: the user's installed TeX Live or MiKTeX.
//!
//! This is the default backend (ADR 0004, amendment of 2026-10-18): it builds
//! with the same toolchain co-authors and journals use. Every run happens in a
//! build directory Manang owns, so aux files never land in the project, and
//! under one deadline that kills the whole process tree when it expires.
//!
//! Hardening, because a project may come from anyone:
//! - shell escape is off (`-no-shell-escape`, `shell_escape=f`);
//! - TeX may only write below its output directory (`openout_any=p`);
//! - `latexmk` runs with `-norc`, so a `latexmkrc` shipped inside a project
//!   (which is Perl) is never executed;
//! - the environment is reduced to an allowlist;
//! - no shell is involved: arguments are passed as an array.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use wait_timeout::ChildExt;

use crate::error::LatexError;

/// A TeX engine the system backend can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Pdflatex,
    Xelatex,
    Lualatex,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Pdflatex, Engine::Xelatex, Engine::Lualatex];

    pub fn program(self) -> &'static str {
        match self {
            Self::Pdflatex => "pdflatex",
            Self::Xelatex => "xelatex",
            Self::Lualatex => "lualatex",
        }
    }

    fn latexmk_flag(self) -> &'static str {
        match self {
            Self::Pdflatex => "-pdf",
            Self::Xelatex => "-xelatex",
            Self::Lualatex => "-lualatex",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.program() == name)
    }
}

/// What the installed TeX distribution offers. Detected once and cached by
/// the plugin; detection is cheap except for one `--version` call.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TexTools {
    pub engines: BTreeMap<Engine, PathBuf>,
    pub latexmk: Option<PathBuf>,
    pub bibtex: Option<PathBuf>,
    pub biber: Option<PathBuf>,
    /// First line of the engine's `--version`, e.g. `pdfTeX 3.14… (TeX Live 2026)`.
    pub version: Option<String>,
}

impl TexTools {
    /// Detect tools on the current `PATH`.
    pub fn detect() -> Self {
        Self::detect_in(std::env::var_os("PATH").as_deref())
    }

    /// Detect tools on the given `PATH` value (tests pass a fake one).
    pub fn detect_in(path: Option<&OsStr>) -> Self {
        let dirs: Vec<PathBuf> = path
            .map(|p| std::env::split_paths(p).collect())
            .unwrap_or_default();
        let find = |name: &str| find_executable(name, &dirs);

        let mut engines = BTreeMap::new();
        for engine in Engine::ALL {
            if let Some(path) = find(engine.program()) {
                engines.insert(engine, path);
            }
        }
        let version = engines
            .values()
            .next()
            .and_then(|program| first_line_of_version(program));
        Self {
            engines,
            latexmk: find("latexmk"),
            bibtex: find("bibtex"),
            biber: find("biber"),
            version,
        }
    }

    pub fn is_available(&self) -> bool {
        !self.engines.is_empty()
    }
}

fn find_executable(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    dirs.iter()
        .map(|dir| dir.join(&file))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// `program --version`, first line, with a short timeout. `None` on any error.
fn first_line_of_version(program: &Path) -> Option<String> {
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // `--version` prints a few hundred bytes, far below a pipe buffer, so
    // waiting before reading cannot deadlock.
    match child.wait_timeout(Duration::from_secs(5)).ok()? {
        Some(_) => {}
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    }
    let mut out = String::new();
    use std::io::Read;
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    out.lines()
        .next()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
}

/// One system build.
#[derive(Debug, Clone)]
pub struct SystemSpec {
    pub engine: Engine,
    /// Use `latexmk` when installed. When `false`, or when it is missing, the
    /// engine and `bibtex` are run directly with up to three engine passes.
    pub latexmk: bool,
    pub synctex: bool,
    pub timeout: Duration,
    /// Embedded as the build date for reproducible PDFs.
    pub build_epoch: u64,
}

/// The result of a system run that reached TeX.
#[derive(Debug, Clone)]
pub struct SystemRun {
    /// File stem of the outputs in the output directory.
    pub stem: String,
    /// The last TeX process exited successfully.
    pub succeeded: bool,
    /// Exit status text when it did not.
    pub failure: Option<String>,
}

/// Upper bounds for mirroring the project's folders into the output directory
/// (pdfLaTeX needs `out/<dir>/` to exist to write `\include` aux files).
const MIRROR_MAX_DIRS: usize = 2_000;
const MIRROR_MAX_DEPTH: usize = 8;

/// Engine passes when running without latexmk.
const MAX_PASSES: usize = 3;

/// Run TeX on `main` (inside `root`), writing every output to `out_dir`.
pub fn run(
    tools: &TexTools,
    main: &Path,
    root: &Path,
    out_dir: &Path,
    spec: &SystemSpec,
) -> Result<SystemRun, LatexError> {
    let engine_path = tools
        .engines
        .get(&spec.engine)
        .ok_or_else(|| LatexError::EngineMissing(spec.engine.program().to_owned()))?;
    let deadline = Instant::now() + spec.timeout;

    let main_rel = main
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| PathBuf::from(main.file_name().unwrap_or(main.as_os_str())));
    let stem = main_rel
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| LatexError::BadArgument("main", "has no file name".to_owned()))?;

    fs::create_dir_all(out_dir).map_err(|e| io_err(out_dir, e))?;
    mirror_dirs(root, out_dir);
    // A persistent build directory may hold an older PDF; never show it as
    // the result of this run.
    let _ = fs::remove_file(out_dir.join(format!("{stem}.pdf")));

    let env = TexEnv::new(root, out_dir, spec.build_epoch);

    if spec.latexmk
        && let Some(latexmk) = &tools.latexmk
    {
        let mut command = env.command(latexmk, root);
        command
            .arg(spec.engine.latexmk_flag())
            .arg("-norc")
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg("-file-line-error")
            .arg("-no-shell-escape")
            .arg(format!("-outdir={}", out_dir.display()));
        if spec.synctex {
            command.arg("-synctex=1");
        }
        command.arg(&main_rel);
        let status = run_until(command, deadline, spec.timeout)?;
        return Ok(SystemRun {
            stem,
            succeeded: status.is_none(),
            failure: status,
        });
    }

    // Manual pipeline: engine, then bibtex when the document asks for it,
    // then reruns until references settle.
    let engine_pass = || {
        let mut command = env.command(engine_path, root);
        command
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg("-file-line-error")
            .arg("-no-shell-escape")
            .arg(format!("-output-directory={}", out_dir.display()));
        if spec.synctex {
            command.arg("-synctex=1");
        }
        command.arg(&main_rel);
        run_until(command, deadline, spec.timeout)
    };

    let mut failure = engine_pass()?;
    if failure.is_none() {
        let aux = fs::read_to_string(out_dir.join(format!("{stem}.aux"))).unwrap_or_default();
        if aux.contains("\\bibdata")
            && let Some(bibtex) = &tools.bibtex
        {
            let mut command = env.command(bibtex, out_dir);
            command.arg(&stem);
            // A BibTeX failure is reported through the .blg log, not as a
            // hard error: the engine pass still produces a PDF.
            let _ = run_until(command, deadline, spec.timeout)?;
            failure = engine_pass()?;
        }
        let mut passes = 1;
        while failure.is_none() && passes < MAX_PASSES && needs_rerun(out_dir, &stem) {
            failure = engine_pass()?;
            passes += 1;
        }
    }
    Ok(SystemRun {
        stem,
        succeeded: failure.is_none(),
        failure,
    })
}

fn needs_rerun(out_dir: &Path, stem: &str) -> bool {
    let log = fs::read_to_string(out_dir.join(format!("{stem}.log"))).unwrap_or_default();
    log.contains("Rerun to get")
        || log.contains("Label(s) may have changed")
        || (log.contains("There were undefined references") && log.contains("Please rerun LaTeX"))
}

/// Recreate the project's folder tree (not files) below `out_dir`, bounded.
fn mirror_dirs(root: &Path, out_dir: &Path) {
    let mut queue = vec![(root.to_path_buf(), 0usize)];
    let mut made = 0usize;
    while let Some((dir, depth)) = queue.pop() {
        if depth >= MIRROR_MAX_DEPTH {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let hidden = name.to_string_lossy().starts_with('.');
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            if hidden || !is_dir {
                continue;
            }
            let path = entry.path();
            if path.starts_with(out_dir) {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(root) {
                let _ = fs::create_dir_all(out_dir.join(rel));
                made += 1;
                if made >= MIRROR_MAX_DIRS {
                    return;
                }
            }
            queue.push((path, depth + 1));
        }
    }
}

/// Environment variables passed through to TeX unchanged.
const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TEMP",
    "TMP",
    "TEXMFHOME",
    "TEXMFVAR",
    "TEXMFCONFIG",
    "TEXMFCACHE",
    "TEXMFLOCAL",
    "TEXINPUTS",
    "BSTINPUTS",
    "OSFONTDIR",
    "SystemRoot",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "MIKTEX_USERCONFIG",
    "MIKTEX_USERDATA",
];

struct TexEnv {
    vars: Vec<(OsString, OsString)>,
}

impl TexEnv {
    fn new(root: &Path, out_dir: &Path, build_epoch: u64) -> Self {
        let mut vars: Vec<(OsString, OsString)> = ENV_ALLOWLIST
            .iter()
            .filter_map(|name| std::env::var_os(name).map(|v| (OsString::from(name), v)))
            .collect();
        // BibTeX runs inside the output directory; let it find the project's
        // .bib files, then fall back to the distribution's search path (the
        // trailing empty entry means "the default path" to kpathsea).
        let mut bib = vec![root.to_path_buf()];
        if let Some(existing) = std::env::var_os("BIBINPUTS") {
            bib.extend(std::env::split_paths(&existing));
        }
        bib.push(PathBuf::new());
        if let Ok(joined) = std::env::join_paths(bib) {
            vars.push(("BIBINPUTS".into(), joined));
        }
        vars.extend([
            ("SOURCE_DATE_EPOCH".into(), build_epoch.to_string().into()),
            ("FORCE_SOURCE_DATE".into(), "1".into()),
            ("openout_any".into(), "p".into()),
            ("shell_escape".into(), "f".into()),
            (
                "TEXMF_OUTPUT_DIRECTORY".into(),
                out_dir.as_os_str().to_owned(),
            ),
        ]);
        Self { vars }
    }

    fn command(&self, program: &Path, cwd: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .env_clear()
            .envs(self.vars.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }
}

/// Run to completion within the deadline. `Ok(None)` on success,
/// `Ok(Some(status))` when the process failed, `Err(Timeout)` when the
/// deadline passed (the process tree is killed).
fn run_until(
    mut command: Command,
    deadline: Instant,
    limit: Duration,
) -> Result<Option<String>, LatexError> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group, so a timeout kills latexmk and the engines it
        // started, not just the parent.
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LatexError::NoTexInstallation
        } else {
            LatexError::WorkerSpawn(e)
        }
    })?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    match child
        .wait_timeout(remaining)
        .map_err(LatexError::WorkerSpawn)?
    {
        Some(status) if status.success() => Ok(None),
        Some(status) => Ok(Some(status.to_string())),
        None => {
            kill_tree(&mut child);
            Err(LatexError::Timeout(limit))
        }
    }
}

#[cfg(unix)]
fn kill_tree(child: &mut Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: `killpg` only sends a signal; `pid` is the group id we
        // created with `process_group(0)` for this child, so no unrelated
        // process group can be targeted. Errors (already exited) are ignored.
        unsafe {
            libc::killpg(pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn kill_tree(child: &mut Child) {
    // Without job objects only the direct child is killed on Windows; latexmk
    // children may linger until they finish. Documented limitation.
    let _ = child.kill();
    let _ = child.wait();
}

fn io_err(path: &Path, source: std::io::Error) -> LatexError {
    LatexError::Io {
        path: path.display().to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines_parse_by_program_name() {
        assert_eq!(Engine::parse("pdflatex"), Some(Engine::Pdflatex));
        assert_eq!(Engine::parse("lualatex"), Some(Engine::Lualatex));
        assert_eq!(Engine::parse("tex"), None);
    }

    #[cfg(unix)]
    #[test]
    fn detects_only_executable_tools_on_the_given_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tmp");
        let make = |name: &str, mode: u32| {
            let p = dir.path().join(name);
            fs::write(&p, "#!/bin/sh\necho 'pdfTeX 9.9 (Fake TeX)'\n").expect("write");
            fs::set_permissions(&p, fs::Permissions::from_mode(mode)).expect("chmod");
        };
        make("pdflatex", 0o755);
        make("latexmk", 0o755);
        make("xelatex", 0o644); // not executable: ignored
        let tools = TexTools::detect_in(Some(dir.path().as_os_str()));
        assert!(tools.engines.contains_key(&Engine::Pdflatex));
        assert!(!tools.engines.contains_key(&Engine::Xelatex));
        assert!(tools.latexmk.is_some());
        assert!(tools.biber.is_none());
        assert_eq!(tools.version.as_deref(), Some("pdfTeX 9.9 (Fake TeX)"));
    }

    #[test]
    fn no_path_means_no_tex() {
        let tools = TexTools::detect_in(None);
        assert!(!tools.is_available());
    }

    #[test]
    fn mirrors_folders_but_not_hidden_ones() {
        let root = tempfile::tempdir().expect("tmp");
        let out = tempfile::tempdir().expect("tmp");
        fs::create_dir_all(root.path().join("chapters/a")).expect("mk");
        fs::create_dir_all(root.path().join(".git/objects")).expect("mk");
        mirror_dirs(root.path(), out.path());
        assert!(out.path().join("chapters/a").is_dir());
        assert!(!out.path().join(".git").exists());
    }
}
