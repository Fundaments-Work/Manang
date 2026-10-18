//! The optional built-in engine: a Tectonic worker the user downloads once.
//!
//! The app never links Tectonic (AGENTS.md 7.4). Instead it can fetch the
//! prebuilt `manang-latex-worker` for this platform, after the user agrees,
//! and run it as a separate process (see `driver.rs`).
//!
//! Trust model: the app embeds `engines/manifest.json`, which lists the exact
//! size and SHA-256 of the binary for each platform. Only the embedded
//! numbers are trusted. The host named by `MANANG_ENGINE_BASE_URL` serves
//! bytes; a compromised host cannot get a different binary installed because
//! the digest of every download is compared with the embedded one before
//! anything is moved into place. A new engine version therefore ships with a
//! new app release (or, later, a signed manifest).
//!
//! Layout under the engines directory:
//!
//! ```text
//! engines/tectonic-0.17.0/manang-latex-worker   the verified binary
//! engines/tectonic-0.17.0/engine.json           what was installed
//! engines/.partial/<file>-<sha8>.part           a resumable download
//! engines/.staging-0.17.0/                      exists only mid-install
//! ```
//!
//! An install either leaves a complete, verified `tectonic-<version>` folder
//! or none: the download goes to `.partial`, is verified, is moved into a
//! staging folder and the folder is renamed into place.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The manifest compiled into the app.
const EMBEDDED_MANIFEST: &str = include_str!("../../../engines/manifest.json");

/// Where engine files are served from, taken from `MANANG_ENGINE_BASE_URL` at
/// build time. The default is a placeholder that never resolves, so a build
/// that forgot to set it cannot download from anywhere (the UI says the engine
/// is not available yet). See `docs/ops/engine-release.md`.
pub const BASE_URL: &str = match option_env!("MANANG_ENGINE_BASE_URL") {
    Some(url) => url,
    None => PLACEHOLDER_BASE_URL,
};

/// The default of [`BASE_URL`]: reserved `.invalid` TLD, never reachable.
pub const PLACEHOLDER_BASE_URL: &str = "https://engines.manang.invalid/engines";

/// Tectonic's package bundle host. Informational: the worker contacts it on
/// the first compile that needs a package (see `docs/ops/engine-release.md`).
const DEFAULT_PACKAGE_HOST: &str = "relay.fullyjustified.net";

const WORKER_STEM: &str = "manang-latex-worker";
const CHUNK: usize = 64 * 1024;
const MAX_ATTEMPTS: u32 = 3;
const MIN_PROGRESS_STEP: Duration = Duration::from_millis(100);

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("downloading the built-in engine needs your consent")]
    ConsentRequired,
    #[error("the built-in engine is not available for {0} yet")]
    PlatformUnsupported(String),
    #[error("the built-in engine has not been published for this build of Manang yet")]
    NotPublished,
    #[error("the engine manifest is invalid: {0}")]
    BadManifest(String),
    #[error("could not reach {host}: {detail}")]
    Network { host: String, detail: String },
    #[error("{host} answered with HTTP status {status}")]
    BadStatus { host: String, status: u16 },
    #[error("the download stopped after {received} of {expected} bytes")]
    Truncated { received: u64, expected: u64 },
    #[error("the download is larger than the {expected} bytes expected")]
    TooLarge { expected: u64 },
    #[error("the downloaded file does not match the expected checksum; nothing was installed")]
    HashMismatch,
    #[error("the download was cancelled")]
    Cancelled,
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("another engine download is already running")]
    Busy,
    #[error("the downloaded engine cannot start on this computer ({0}); nothing was installed")]
    WontRun(String),
}

impl EngineError {
    /// Stable code for the `[code] message` convention of plugin errors.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConsentRequired => "consent_required",
            Self::PlatformUnsupported(_) => "engine_platform_unsupported",
            Self::NotPublished => "engine_not_published",
            Self::BadManifest(_) => "engine_manifest",
            Self::Network { .. } => "engine_network",
            Self::BadStatus { .. } => "engine_network",
            Self::Truncated { .. } => "engine_truncated",
            Self::TooLarge { .. } => "engine_too_large",
            Self::HashMismatch => "engine_hash_mismatch",
            Self::Cancelled => "engine_cancelled",
            Self::Io { .. } => "io",
            Self::Busy => "engine_busy",
            Self::WontRun(_) => "engine_wont_run",
        }
    }
}

fn io_err(path: &Path, source: io::Error) -> EngineError {
    EngineError::Io {
        path: path.display().to_string(),
        source,
    }
}

// ---------------------------------------------------------------- manifest

/// One platform's entry in `engines/manifest.json`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlatformEntry {
    /// `built` entries have a size and checksum; `planned` ones do not.
    pub status: String,
    pub file: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// The release manifest.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EngineManifest {
    pub schema: u32,
    pub engine: String,
    pub version: String,
    /// Host of the Tectonic package bundle, for the consent text.
    #[serde(default)]
    pub package_host: Option<String>,
    pub platforms: BTreeMap<String, PlatformEntry>,
}

impl EngineManifest {
    pub fn parse(json: &str) -> Result<Self, EngineError> {
        let manifest: Self =
            serde_json::from_str(json).map_err(|e| EngineError::BadManifest(e.to_string()))?;
        if manifest.schema != 1 {
            return Err(EngineError::BadManifest(format!(
                "unsupported schema {}",
                manifest.schema
            )));
        }
        // The version becomes a folder name: keep it to plain characters.
        let plain = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        };
        if !plain(&manifest.version) {
            return Err(EngineError::BadManifest("bad version".to_owned()));
        }
        for (platform, entry) in &manifest.platforms {
            let name_ok = !entry.file.is_empty()
                && !entry.file.contains(['/', '\\'])
                && !entry.file.starts_with('.');
            if !name_ok {
                return Err(EngineError::BadManifest(format!(
                    "{platform}: bad file name"
                )));
            }
            if entry.status == "built" {
                let hash_ok = entry.sha256.as_deref().is_some_and(is_sha256_hex);
                if entry.size.is_none() || !hash_ok {
                    return Err(EngineError::BadManifest(format!(
                        "{platform}: a built entry needs `size` and a 64-digit `sha256`"
                    )));
                }
            }
        }
        Ok(manifest)
    }

    /// The manifest embedded in this build.
    pub fn embedded() -> Result<Self, EngineError> {
        Self::parse(EMBEDDED_MANIFEST)
    }
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `linux-x86_64`, `macos-aarch64`, `windows-x86_64`...
pub fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// What to install: one platform's entry, resolved against a base URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnginePlan {
    pub version: String,
    pub platform: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
    /// Host the file comes from, for the consent text.
    pub host: String,
    /// Host of the Tectonic package bundle, for the consent text.
    pub package_host: String,
}

/// The manifest, base URL and platform an install resolves against. Tests
/// substitute their own; the app uses [`EngineSource::embedded`].
#[derive(Debug, Clone)]
pub struct EngineSource {
    pub manifest: EngineManifest,
    pub base_url: String,
    pub platform: String,
}

impl EngineSource {
    pub fn embedded() -> Result<Self, EngineError> {
        Ok(Self {
            manifest: EngineManifest::embedded()?,
            base_url: BASE_URL.to_owned(),
            platform: current_platform(),
        })
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// Why nothing can be downloaded on this build, if so.
    pub fn plan(&self) -> Result<EnginePlan, EngineError> {
        let entry = self
            .manifest
            .platforms
            .get(&self.platform)
            .filter(|e| e.status == "built")
            .ok_or_else(|| EngineError::PlatformUnsupported(self.platform.clone()))?;
        let (Some(size), Some(sha256)) = (entry.size, entry.sha256.clone()) else {
            return Err(EngineError::PlatformUnsupported(self.platform.clone()));
        };
        let base = self.base_url.trim_end_matches('/');
        let host = url_host(base).ok_or(EngineError::NotPublished)?;
        let bare = host.rsplit_once(':').map_or(host.as_str(), |(h, port)| {
            if port.bytes().all(|b| b.is_ascii_digit()) {
                h
            } else {
                host.as_str()
            }
        });
        if bare.ends_with(".invalid") {
            return Err(EngineError::NotPublished);
        }
        // Only https, except loopback (tests and local mirrors).
        let secure = base.starts_with("https://")
            || (base.starts_with("http://")
                && (bare == "127.0.0.1" || bare == "localhost" || bare == "[::1]"));
        if !secure {
            return Err(EngineError::BadManifest(
                "the engine base URL must be https".to_owned(),
            ));
        }
        Ok(EnginePlan {
            version: self.manifest.version.clone(),
            platform: self.platform.clone(),
            url: format!("{base}/tectonic-{}/{}", self.manifest.version, entry.file),
            file: entry.file.clone(),
            size,
            sha256: sha256.to_ascii_lowercase(),
            host,
            package_host: self
                .manifest
                .package_host
                .clone()
                .unwrap_or_else(|| DEFAULT_PACKAGE_HOST.to_owned()),
        })
    }
}

/// `host[:port]` of an http(s) URL, without userinfo.
fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let bare = host.split(':').next().unwrap_or(host);
    (!bare.is_empty()).then(|| host.to_owned())
}

// ------------------------------------------------------------------ fetching

/// One HTTP response body, possibly a continuation of an earlier download.
pub struct Download {
    pub body: Box<dyn Read + Send>,
    /// `true` when the server honoured the `Range` request: the body starts
    /// at the requested offset. `false` means it starts at byte 0.
    pub resumed: bool,
}

/// Fetches bytes. A trait so tests use a local server (or a scripted fake)
/// and a future browser build can use `fetch`.
pub trait Fetcher: Send + Sync {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, EngineError>;
}

/// The production fetcher: ureq with rustls on ring (no OpenSSL, no aws-lc).
/// Compiled with feature `download` (on by default; the worker build leaves
/// it off so the sidecar carries no HTTP stack of its own).
#[cfg(feature = "download")]
pub struct HttpFetcher;

#[cfg(feature = "download")]
impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, offset: u64) -> Result<Download, EngineError> {
        let host = url_host(url).unwrap_or_default();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
            .user_agent("manang-engine-installer")
            .build()
            .into();
        let mut request = agent.get(url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request.call().map_err(|e| EngineError::Network {
            host: host.clone(),
            detail: e.to_string(),
        })?;
        let status = response.status().as_u16();
        let resumed = match status {
            200 => false,
            206 => {
                // Only continue when the server starts where we asked.
                let start = response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("bytes "))
                    .and_then(|v| v.split('-').next())
                    .and_then(|v| v.parse::<u64>().ok());
                if start != Some(offset) {
                    return Err(EngineError::Network {
                        host,
                        detail: "the server resumed at the wrong position".to_owned(),
                    });
                }
                true
            }
            other => {
                return Err(EngineError::BadStatus {
                    host,
                    status: other,
                });
            }
        };
        Ok(Download {
            body: Box::new(response.into_body().into_reader()),
            resumed,
        })
    }
}

// ------------------------------------------------------------------- install

/// Where an install is, for the progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Downloading,
    Verifying,
    Installing,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub received: u64,
    pub total: u64,
}

/// The marker written next to an installed worker.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct InstalledEngine {
    pub version: String,
    pub platform: String,
    pub size: u64,
    pub sha256: String,
}

/// The engines directory (`<app data>/engines`).
#[derive(Debug, Clone)]
pub struct EngineStore {
    root: PathBuf,
}

/// The consent the caller obtained. Constructed only from an explicit
/// `true`, so no code path starts a download without it.
#[derive(Debug, Clone, Copy)]
pub struct Consent(());

impl Consent {
    pub fn from_flag(granted: bool) -> Result<Self, EngineError> {
        granted
            .then_some(Self(()))
            .ok_or(EngineError::ConsentRequired)
    }
}

impl EngineStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn version_dir(&self, version: &str) -> PathBuf {
        self.root.join(format!("tectonic-{version}"))
    }

    /// Path of the worker of `version`, whether or not it exists.
    pub fn worker_path(&self, version: &str) -> PathBuf {
        self.version_dir(version)
            .join(format!("{WORKER_STEM}{}", std::env::consts::EXE_SUFFIX))
    }

    /// The installed engine for `plan`, when a complete verified install is
    /// there: marker matches the embedded digest and the file has the size.
    pub fn installed(&self, plan: &EnginePlan) -> Option<InstalledEngine> {
        let marker = fs::read(self.version_dir(&plan.version).join("engine.json")).ok()?;
        let marker: InstalledEngine = serde_json::from_slice(&marker).ok()?;
        let meta = fs::metadata(self.worker_path(&plan.version)).ok()?;
        (marker.sha256 == plan.sha256 && marker.size == plan.size && meta.len() == plan.size)
            .then_some(marker)
    }

    /// The installed worker's path and version for `source`, if any.
    pub fn installed_worker(&self, source: &EngineSource) -> Option<(PathBuf, InstalledEngine)> {
        let plan = source.plan().ok()?;
        let marker = self.installed(&plan)?;
        Some((self.worker_path(&plan.version), marker))
    }

    fn partial_path(&self, plan: &EnginePlan) -> PathBuf {
        let short = plan.sha256.get(..8).unwrap_or("00000000");
        self.root
            .join(".partial")
            .join(format!("{}-{short}.part", plan.file))
    }

    /// Delete the installed engine and any partial download. Idempotent.
    pub fn remove(&self) -> Result<(), EngineError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io_err(&self.root, e)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("tectonic-") || name.starts_with(".staging-") || name == ".partial"
            {
                let path = entry.path();
                fs::remove_dir_all(&path).map_err(|e| io_err(&path, e))?;
            }
        }
        // Leave an empty engines folder behind? No: remove it when empty.
        let _ = fs::remove_dir(&self.root);
        Ok(())
    }

    /// Download, verify and install `plan`. See the module docs for the
    /// guarantees. Blocks; call it from a worker thread. `progress` is called
    /// at most every 100 ms while downloading, plus at each phase change.
    pub fn install(
        &self,
        plan: &EnginePlan,
        consent: Consent,
        fetcher: &dyn Fetcher,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<InstalledEngine, EngineError> {
        let Consent(()) = consent;
        let total = plan.size;
        if let Some(done) = self.installed(plan) {
            progress(Progress {
                phase: Phase::Done,
                received: total,
                total,
            });
            return Ok(done);
        }
        let partial_dir = self.root.join(".partial");
        fs::create_dir_all(&partial_dir).map_err(|e| io_err(&partial_dir, e))?;
        let part = self.partial_path(plan);

        let mut last_error = EngineError::Truncated {
            received: 0,
            expected: total,
        };
        let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if have > total {
            fs::remove_file(&part).map_err(|e| io_err(&part, e))?;
            have = 0;
        }
        for _ in 0..MAX_ATTEMPTS {
            if have == total {
                break;
            }
            match self.download_once(plan, fetcher, cancel, progress, &part, have) {
                Ok(()) => {}
                Err(EngineError::Cancelled) => {
                    // The user said stop: leave nothing behind.
                    let _ = fs::remove_file(&part);
                    return Err(EngineError::Cancelled);
                }
                // Only network-flavoured failures are worth another try; the
                // partial file stays so the next attempt continues from it.
                Err(
                    e @ (EngineError::Truncated { .. }
                    | EngineError::Network { .. }
                    | EngineError::BadStatus { .. }),
                ) => last_error = e,
                Err(e) => return Err(e),
            }
            have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            if have == total {
                break;
            }
        }
        if have != total {
            return Err(match last_error {
                EngineError::Truncated { expected, .. } => EngineError::Truncated {
                    received: have,
                    expected,
                },
                other => other,
            });
        }

        progress(Progress {
            phase: Phase::Verifying,
            received: total,
            total,
        });
        if sha256_of(&part)? != plan.sha256 {
            // Never keep a file that failed verification.
            let _ = fs::remove_file(&part);
            return Err(EngineError::HashMismatch);
        }

        progress(Progress {
            phase: Phase::Installing,
            received: total,
            total,
        });
        let marker = self.place(plan, &part)?;
        progress(Progress {
            phase: Phase::Done,
            received: total,
            total,
        });
        Ok(marker)
    }

    /// One HTTP attempt, appending to `part` from byte `have`.
    fn download_once(
        &self,
        plan: &EnginePlan,
        fetcher: &dyn Fetcher,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
        part: &Path,
        have: u64,
    ) -> Result<(), EngineError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::Cancelled);
        }
        let mut download = fetcher.fetch(&plan.url, have)?;
        let mut file = if have > 0 && download.resumed {
            fs::OpenOptions::new()
                .append(true)
                .open(part)
                .map_err(|e| io_err(part, e))?
        } else {
            // A fresh start (or the server ignored the Range header).
            fs::File::create(part).map_err(|e| io_err(part, e))?
        };
        let mut received = if download.resumed { have } else { 0 };
        let mut buf = vec![0u8; CHUNK];
        let mut last_emit = std::time::Instant::now() - MIN_PROGRESS_STEP;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(EngineError::Cancelled);
            }
            let n = match download.body.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                // A dropped connection looks like a short body: keep what we
                // have and let the caller resume.
                Err(_) => break,
            };
            // Never accept more than the manifest says.
            if received + n as u64 > plan.size {
                drop(file);
                let _ = fs::remove_file(part);
                return Err(EngineError::TooLarge {
                    expected: plan.size,
                });
            }
            file.write_all(&buf[..n]).map_err(|e| io_err(part, e))?;
            received += n as u64;
            if last_emit.elapsed() >= MIN_PROGRESS_STEP {
                last_emit = std::time::Instant::now();
                progress(Progress {
                    phase: Phase::Downloading,
                    received,
                    total: plan.size,
                });
            }
        }
        file.flush().map_err(|e| io_err(part, e))?;
        progress(Progress {
            phase: Phase::Downloading,
            received,
            total: plan.size,
        });
        if received < plan.size {
            return Err(EngineError::Truncated {
                received,
                expected: plan.size,
            });
        }
        Ok(())
    }

    /// Move the verified file into a staging folder and rename that folder
    /// into place.
    fn place(&self, plan: &EnginePlan, part: &Path) -> Result<InstalledEngine, EngineError> {
        let staging = self.root.join(format!(".staging-{}", plan.version));
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|e| io_err(&staging, e))?;
        }
        fs::create_dir_all(&staging).map_err(|e| io_err(&staging, e))?;
        let result = self.place_in(plan, part, &staging);
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    fn place_in(
        &self,
        plan: &EnginePlan,
        part: &Path,
        staging: &Path,
    ) -> Result<InstalledEngine, EngineError> {
        let worker = staging.join(format!("{WORKER_STEM}{}", std::env::consts::EXE_SUFFIX));
        fs::rename(part, &worker).map_err(|e| io_err(&worker, e))?;
        make_executable(&worker)?;
        fs::File::open(&worker)
            .and_then(|f| f.sync_all())
            .map_err(|e| io_err(&worker, e))?;
        smoke_test(&worker)?;
        let marker = InstalledEngine {
            version: plan.version.clone(),
            platform: plan.platform.clone(),
            size: plan.size,
            sha256: plan.sha256.clone(),
        };
        let marker_path = staging.join("engine.json");
        let json = serde_json::to_vec_pretty(&marker)
            .map_err(|e| io_err(&marker_path, io::Error::other(e)))?;
        fs::write(&marker_path, json).map_err(|e| io_err(&marker_path, e))?;

        let target = self.version_dir(&plan.version);
        // A broken earlier install is replaced; a good one never gets here.
        if target.exists() {
            fs::remove_dir_all(&target).map_err(|e| io_err(&target, e))?;
        }
        fs::rename(staging, &target).map_err(|e| io_err(&target, e))?;
        self.prune_other_versions(&plan.version);
        Ok(marker)
    }

    /// Old engine folders are dead weight once a newer one is in place.
    fn prune_other_versions(&self, keep: &str) {
        let keep = format!("tectonic-{keep}");
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("tectonic-") && name != keep {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

/// Run `worker --version`: a binary linked against system libraries this
/// computer lacks fails here, with the loader's message, instead of at the
/// user's first compile.
fn smoke_test(worker: &Path) -> Result<(), EngineError> {
    // A file that was just written can briefly be "busy" (ETXTBSY, errno 26)
    // when another thread forks while its write handle is open: the child
    // holds a copy until it execs. It clears in microseconds; retry.
    let mut attempts = 0;
    let mut child = loop {
        let spawned = std::process::Command::new(worker)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();
        match spawned {
            Ok(child) => break child,
            Err(e) if e.raw_os_error() == Some(26) && attempts < 50 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(EngineError::WontRun(e.to_string())),
        }
    };
    let status = wait_timeout::ChildExt::wait_timeout(&mut child, Duration::from_secs(15))
        .map_err(|e| EngineError::WontRun(e.to_string()))?;
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(EngineError::WontRun("it did not answer in time".to_owned()));
    };
    if status.success() {
        return Ok(());
    }
    let mut detail = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.by_ref().take(600).read_to_string(&mut detail);
    }
    Err(EngineError::WontRun(detail.trim().to_owned()))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), EngineError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| io_err(path, e))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), EngineError> {
    Ok(())
}

/// Lower-case hex SHA-256 of a file, streamed.
pub fn sha256_of(path: &Path) -> Result<String, EngineError> {
    let mut file = fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(|e| io_err(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Lower-case hex SHA-256 of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Used when the `download` feature is off: every fetch fails clearly.
#[cfg(not(feature = "download"))]
pub struct HttpFetcher;

#[cfg(not(feature = "download"))]
impl Fetcher for HttpFetcher {
    fn fetch(&self, _url: &str, _offset: u64) -> Result<Download, EngineError> {
        Err(EngineError::Network {
            host: String::new(),
            detail: "this build has no download support".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_is_valid_and_lists_every_platform() {
        let manifest = EngineManifest::embedded().expect("embedded manifest parses");
        assert_eq!(manifest.engine, "tectonic");
        for platform in [
            "linux-x86_64",
            "linux-aarch64",
            "windows-x86_64",
            "macos-aarch64",
            "macos-x86_64",
        ] {
            assert!(manifest.platforms.contains_key(platform), "{platform}");
        }
    }

    #[test]
    fn placeholder_base_url_is_never_downloadable() {
        let mut source = EngineSource {
            manifest: EngineManifest::embedded().expect("manifest"),
            base_url: PLACEHOLDER_BASE_URL.to_owned(),
            platform: "linux-x86_64".to_owned(),
        };
        assert!(matches!(source.plan(), Err(EngineError::NotPublished)));
        source.base_url = "http://example.com/x".to_owned();
        assert!(matches!(source.plan(), Err(EngineError::BadManifest(_))));
    }

    #[test]
    fn planned_platforms_have_no_plan() {
        let source = EngineSource {
            manifest: EngineManifest::embedded().expect("manifest"),
            base_url: "https://example.com/e".to_owned(),
            platform: "macos-aarch64".to_owned(),
        };
        assert!(matches!(
            source.plan(),
            Err(EngineError::PlatformUnsupported(_))
        ));
    }

    #[test]
    fn built_entries_need_a_real_digest() {
        let bad = r#"{"schema":1,"engine":"tectonic","version":"1","platforms":
            {"linux-x86_64":{"status":"built","file":"w","size":1,"sha256":"abc"}}}"#;
        assert!(EngineManifest::parse(bad).is_err());
        let traversal = r#"{"schema":1,"engine":"tectonic","version":"1","platforms":
            {"linux-x86_64":{"status":"planned","file":"../w"}}}"#;
        assert!(EngineManifest::parse(traversal).is_err());
    }

    #[test]
    fn host_is_read_without_userinfo() {
        assert_eq!(
            url_host("https://user@example.com:8443/x").as_deref(),
            Some("example.com:8443")
        );
        assert_eq!(url_host("ftp://x"), None);
    }

    #[test]
    fn consent_must_be_an_explicit_yes() {
        assert!(matches!(
            Consent::from_flag(false),
            Err(EngineError::ConsentRequired)
        ));
        assert!(Consent::from_flag(true).is_ok());
    }
}
