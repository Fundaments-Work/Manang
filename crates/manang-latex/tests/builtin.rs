//! The consented download of the built-in engine, against a local HTTP server.
//!
//! The "engine" served here is a small shell script that answers `--version`
//! and writes a fake PDF, so the whole path (download, verify, install, find,
//! run) is exercised without Tectonic. Unix only: the stand-in is a script.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use manang_kernel::{EventBus, Kernel, Payload};
use manang_latex::builtin::{
    Consent, EngineError, EngineManifest, EngineSource, EngineStore, HttpFetcher, Phase,
    PlatformEntry, sha256_hex,
};
use manang_latex::{BackendChoice, LatexConfig, LatexPlugin, TexTools, manifest};
use parking_lot::Mutex;
use serde_json::{Value, json};

/// A stand-in worker. `--version` succeeds; otherwise it writes the outputs a
/// compile would, to the `out_dir` named in the request file.
const FAKE_WORKER: &str = r##"#!/bin/sh
[ "$1" = "--version" ] && { echo "manang-latex-worker test"; exit 0; }
out=$(sed -n 's/.*"out_dir":"\([^"]*\)".*/\1/p' "$1")
printf '{"failure":null,"message":"","stem":"main"}' > "$out/outcome.json"
printf '%%PDF-1.4 fake' > "$out/main.pdf"
printf 'Output written on main.pdf (1 page, 14 bytes).\n' > "$out/main.log"
"##;

/// How the local server misbehaves.
#[derive(Clone, Copy)]
enum Mode {
    Normal,
    /// Send only this many body bytes, then close (every request).
    TruncateAlways(usize),
    /// Truncate the first request only.
    TruncateFirst(usize),
    /// Ignore `Range` and always answer 200 with the whole body.
    IgnoreRange,
}

struct Server {
    addr: String,
    requests: Arc<AtomicUsize>,
    ranges: Arc<Mutex<Vec<Option<u64>>>>,
    mode: Arc<Mutex<Mode>>,
}

fn serve(body: Vec<u8>, mode: Mode) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = format!("127.0.0.1:{}", listener.local_addr().expect("addr").port());
    let requests = Arc::new(AtomicUsize::new(0));
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let mode = Arc::new(Mutex::new(mode));
    let (r, g, m) = (
        Arc::clone(&requests),
        Arc::clone(&ranges),
        Arc::clone(&mode),
    );
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let n = r.fetch_add(1, Ordering::SeqCst);
            let mode = *m.lock();
            handle(stream, &body, mode, n, &g);
        }
    });
    Server {
        addr,
        requests,
        ranges,
        mode,
    }
}

fn handle(
    mut stream: TcpStream,
    body: &[u8],
    mode: Mode,
    nth: usize,
    ranges: &Mutex<Vec<Option<u64>>>,
) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let start = head
        .lines()
        .find_map(|l| l.strip_prefix("range: bytes="))
        .and_then(|v| v.trim_end_matches('-').parse::<u64>().ok());
    ranges.lock().push(start);
    let honour_range = !matches!(mode, Mode::IgnoreRange);
    let from = if honour_range {
        usize::try_from(start.unwrap_or(0)).unwrap_or(0)
    } else {
        0
    };
    let slice = &body[from.min(body.len())..];
    let (status, extra) = if honour_range && start.is_some() {
        (
            "206 Partial Content",
            format!(
                "Content-Range: bytes {from}-{}/{}\r\n",
                body.len() - 1,
                body.len()
            ),
        )
    } else {
        ("200 OK", String::new())
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        slice.len()
    );
    let cut = match mode {
        Mode::TruncateAlways(n) => Some(n),
        Mode::TruncateFirst(n) if nth == 0 => Some(n),
        _ => None,
    };
    let send = cut.map_or(slice.len(), |n| n.min(slice.len()));
    let _ = stream.write_all(&slice[..send]);
    let _ = stream.flush();
}

fn manifest_for(bytes: &[u8]) -> EngineManifest {
    let mut platforms = BTreeMap::new();
    platforms.insert(
        "test-platform".to_owned(),
        PlatformEntry {
            status: "built".to_owned(),
            file: "worker-test".to_owned(),
            size: Some(bytes.len() as u64),
            sha256: Some(sha256_hex(bytes)),
        },
    );
    EngineManifest {
        schema: 1,
        engine: "tectonic".to_owned(),
        version: "9.9.9".to_owned(),
        package_host: Some("packages.example.test".to_owned()),
        platforms,
    }
}

fn source_for(server: &Server, bytes: &[u8]) -> EngineSource {
    EngineSource {
        manifest: manifest_for(bytes),
        base_url: format!("http://{}/engines", server.addr),
        platform: "test-platform".to_owned(),
    }
}

/// A body that is as long as the real one and wrong.
fn corrupt(bytes: &[u8]) -> Vec<u8> {
    let mut bad = bytes.to_vec();
    if let Some(last) = bad.last_mut() {
        *last ^= 0xff;
    }
    bad
}

fn install(
    store: &EngineStore,
    source: &EngineSource,
) -> (
    Result<(), EngineError>,
    Vec<manang_latex::builtin::Progress>,
) {
    let plan = source.plan().expect("plan");
    let mut seen = Vec::new();
    let cancel = AtomicBool::new(false);
    let result = store
        .install(
            &plan,
            Consent::from_flag(true).expect("consent"),
            &HttpFetcher,
            &cancel,
            &mut |p| seen.push(p),
        )
        .map(|_| ());
    (result, seen)
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn installs_a_verified_engine_atomically_and_reports_progress() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));

    let (result, progress) = install(&store, &source);
    result.expect("installs");

    let plan = source.plan().expect("plan");
    let marker = store.installed(&plan).expect("installed");
    assert_eq!(marker.sha256, plan.sha256);
    let worker = store.worker_path("9.9.9");
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
        std::fs::metadata(&worker)
            .expect("meta")
            .permissions()
            .mode()
            & 0o111,
        0,
        "marked executable"
    );
    // Nothing but the finished folder is left: no staging, no partial file.
    let names = entries(store.root());
    assert!(names.contains(&"tectonic-9.9.9".to_owned()), "{names:?}");
    assert!(
        !names.iter().any(|n| n.starts_with(".staging")),
        "{names:?}"
    );
    assert!(entries(&store.root().join(".partial")).is_empty());
    assert_eq!(progress.last().map(|p| p.phase), Some(Phase::Done));
    assert!(progress.iter().any(|p| p.phase == Phase::Verifying));

    // Installing again is a no-op: no second download.
    let before = server.requests.load(Ordering::SeqCst);
    install(&store, &source).0.expect("idempotent");
    assert_eq!(server.requests.load(Ordering::SeqCst), before);
}

#[test]
fn a_wrong_hash_is_rejected_and_nothing_is_installed() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    // The host serves bytes of the right length that are not the engine.
    let server = serve(corrupt(&bytes), Mode::Normal);
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));

    let (result, _) = install(&store, &source);
    assert!(
        matches!(result, Err(EngineError::HashMismatch)),
        "{result:?}"
    );
    let plan = source.plan().expect("plan");
    assert!(store.installed(&plan).is_none());
    let names = entries(store.root());
    assert!(
        !names.iter().any(|n| n.starts_with("tectonic-")),
        "{names:?}"
    );
    assert!(
        entries(&store.root().join(".partial")).is_empty(),
        "a failed file is not kept"
    );
}

#[test]
fn an_engine_that_cannot_start_is_not_installed() {
    // Right hash, but not something the system can run.
    let bytes = b"this is not an executable".to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));

    let (result, _) = install(&store, &source);
    assert!(matches!(result, Err(EngineError::WontRun(_))), "{result:?}");
    let names = entries(store.root());
    assert!(
        !names.iter().any(|n| n.starts_with("tectonic-")),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with(".staging")),
        "{names:?}"
    );
}

#[test]
fn a_truncated_download_installs_nothing_and_keeps_a_resumable_part() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::TruncateAlways(40));
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));

    let (result, _) = install(&store, &source);
    assert!(
        matches!(result, Err(EngineError::Truncated { .. })),
        "{result:?}"
    );
    assert!(store.installed(&source.plan().expect("plan")).is_none());
    let names = entries(store.root());
    assert!(
        !names.iter().any(|n| n.starts_with("tectonic-")),
        "{names:?}"
    );
    // The attempts continue where the last one stopped, so the part grew.
    let part = entries(&store.root().join(".partial"));
    assert_eq!(part.len(), 1);
}

#[test]
fn an_interrupted_download_resumes_from_where_it_stopped() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::TruncateFirst(50));
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));

    install(&store, &source).0.expect("retry inside the call");
    assert!(store.installed(&source.plan().expect("plan")).is_some());
    let ranges = server.ranges.lock().clone();
    assert_eq!(ranges.first(), Some(&None));
    assert_eq!(ranges.get(1), Some(&Some(50)), "{ranges:?}");
}

#[test]
fn a_later_call_resumes_a_part_left_by_an_earlier_failure() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::TruncateAlways(60));
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));
    assert!(install(&store, &source).0.is_err());

    // The network comes back.
    *server.mode.lock() = Mode::Normal;
    server.ranges.lock().clear();
    install(&store, &source).0.expect("resumes");
    assert!(store.installed(&source.plan().expect("plan")).is_some());
    let first = server.ranges.lock().first().cloned().flatten();
    assert!(first.is_some_and(|n| n >= 60), "{first:?}");
}

#[test]
fn a_server_that_ignores_range_is_restarted_not_corrupted() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::TruncateAlways(40));
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));
    assert!(install(&store, &source).0.is_err());

    *server.mode.lock() = Mode::IgnoreRange;
    install(&store, &source).0.expect("restarts from zero");
    assert!(store.installed(&source.plan().expect("plan")).is_some());
}

#[test]
fn a_body_longer_than_the_manifest_is_refused() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let mut long = bytes.clone();
    long.extend_from_slice(b"extra");
    let server = serve(long, Mode::Normal);
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));
    let (result, _) = install(&store, &source);
    assert!(
        matches!(result, Err(EngineError::TooLarge { .. })),
        "{result:?}"
    );
    assert!(store.installed(&source.plan().expect("plan")).is_none());
}

#[test]
fn cancelling_leaves_nothing_behind() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));
    let plan = source.plan().expect("plan");
    let cancel = AtomicBool::new(true);
    let result = store.install(
        &plan,
        Consent::from_flag(true).expect("consent"),
        &HttpFetcher,
        &cancel,
        &mut |_| {},
    );
    assert!(matches!(result, Err(EngineError::Cancelled)));
    let names = entries(store.root());
    assert!(
        !names.iter().any(|n| n.starts_with("tectonic-")),
        "{names:?}"
    );
}

#[test]
fn remove_deletes_the_engine_and_is_idempotent() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let source = source_for(&server, &bytes);
    let dir = tempfile::tempdir().expect("tmp");
    let store = EngineStore::new(dir.path().join("engines"));
    install(&store, &source).0.expect("installs");
    store.remove().expect("removes");
    assert!(store.installed(&source.plan().expect("plan")).is_none());
    assert!(!store.version_dir("9.9.9").exists());
    store.remove().expect("removing nothing is fine");
}

// ----------------------------------------------------------- through the plugin

fn plugin_kernel(server: &Server, bytes: &[u8], root: &Path, events: &EventBus) -> Kernel {
    let config = LatexConfig {
        engines_dir: Some(root.join("engines")),
        engine_source: Some(source_for(server, bytes)),
        cache_dir: Some(root.join("cache")),
        ..LatexConfig::default()
    };
    // No system TeX is "installed" for these tests.
    let plugin = LatexPlugin::with_tools(config, TexTools::default()).with_events(events.clone());
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest"), Arc::new(plugin))
        .expect("register");
    kernel
}

fn invoke(kernel: &Kernel, command: &str, args: Value) -> Result<Value, String> {
    match kernel.invoke(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(other) => panic!("expected JSON, got {other:?}"),
        Err(e) => Err(e.to_string()),
    }
}

#[test]
fn the_app_never_starts_a_download_without_consent() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let dir = tempfile::tempdir().expect("tmp");
    let k = plugin_kernel(&server, &bytes, dir.path(), &EventBus::new());

    for args in [
        json!({}),
        json!({ "consent": false }),
        json!({ "consent": "yes" }),
        json!({ "consent": 1 }),
        Value::Null,
    ] {
        let err = invoke(&k, "latex.engine-install", args).expect_err("refused");
        assert!(err.contains("[consent_required]"), "{err}");
    }
    // Asking about the engine, or compiling without one, downloads nothing.
    invoke(&k, "latex.engine-status", Value::Null).expect("status");
    invoke(&k, "latex.status", Value::Null).expect("status");
    let compile = invoke(
        &k,
        "latex.compile",
        json!({ "main": "main.tex", "sources": { "main.tex": "x" } }),
    )
    .expect_err("no engine");
    assert!(compile.contains("[no_tex]"), "{compile}");
    assert_eq!(server.requests.load(Ordering::SeqCst), 0, "no request made");
    assert!(!dir.path().join("engines").exists(), "nothing written");
}

#[test]
fn install_through_the_plugin_makes_auto_compile_with_the_built_in_engine() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let dir = tempfile::tempdir().expect("tmp");
    let events = EventBus::new();
    let progress = events.subscribe("latex.engine-progress");
    let k = plugin_kernel(&server, &bytes, dir.path(), &events);

    let before = invoke(&k, "latex.engine-status", Value::Null).expect("status");
    assert_eq!(before["installed"], false);
    assert_eq!(before["downloadable"], true);
    assert_eq!(before["size"], bytes.len());
    assert_eq!(before["package_host"], "packages.example.test");
    assert_eq!(before["url_host"], server.addr.as_str());
    assert_eq!(
        before["sha256"].as_str().map(str::to_owned),
        Some(sha256_hex(&bytes))
    );
    let top = invoke(&k, "latex.status", Value::Null).expect("status");
    assert_eq!(top["default"], "none");

    let after = invoke(&k, "latex.engine-install", json!({ "consent": true })).expect("install");
    assert_eq!(after["installed"], true);
    let events: Vec<_> = std::iter::from_fn(|| progress.try_recv()).collect();
    assert!(events.iter().any(|e| matches!(&e.payload,
        Payload::Json(v) if v["phase"] == "done")));

    // Backend auto: no system TeX, so the installed engine compiles.
    let top = invoke(&k, "latex.status", Value::Null).expect("status");
    assert_eq!(top["default"], "tectonic");
    let report = invoke(
        &k,
        "latex.compile",
        json!({ "main": "main.tex", "sources": { "main.tex": "x" } }),
    )
    .expect("compiles");
    assert_eq!(report["ok"], true);
    assert_eq!(report["backend"], "tectonic");
    assert_eq!(report["engine"], "xelatex");
    assert_eq!(report["tex_version"], "Tectonic 9.9.9");

    // Remove: back to nothing, and compiling says so.
    let gone = invoke(&k, "latex.engine-remove", Value::Null).expect("remove");
    assert_eq!(gone["installed"], false);
    let err = invoke(
        &k,
        "latex.compile",
        json!({ "main": "main.tex", "sources": { "main.tex": "x" } }),
    )
    .expect_err("no engine");
    assert!(err.contains("[no_tex]"), "{err}");
}

#[test]
fn a_system_tex_wins_over_the_installed_engine_in_auto() {
    let bytes = FAKE_WORKER.as_bytes().to_vec();
    let server = serve(bytes.clone(), Mode::Normal);
    let dir = tempfile::tempdir().expect("tmp");
    let config = LatexConfig {
        engines_dir: Some(dir.path().join("engines")),
        engine_source: Some(source_for(&server, &bytes)),
        backend: BackendChoice::Auto,
        ..LatexConfig::default()
    };
    let mut tools = TexTools::default();
    tools.engines.insert(
        manang_latex::Engine::Pdflatex,
        std::path::PathBuf::from("/usr/bin/pdflatex"),
    );
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest"),
            Arc::new(LatexPlugin::with_tools(config, tools)),
        )
        .expect("register");
    let top = invoke(&kernel, "latex.status", Value::Null).expect("status");
    assert_eq!(top["default"], "system");
}

#[test]
fn an_unpublished_base_url_offers_no_download() {
    let source = EngineSource {
        manifest: EngineManifest::embedded().expect("embedded"),
        base_url: manang_latex::builtin::PLACEHOLDER_BASE_URL.to_owned(),
        platform: "linux-x86_64".to_owned(),
    };
    let dir = tempfile::tempdir().expect("tmp");
    let config = LatexConfig {
        engines_dir: Some(dir.path().join("engines")),
        engine_source: Some(source),
        ..LatexConfig::default()
    };
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest"),
            Arc::new(LatexPlugin::with_tools(config, TexTools::default())),
        )
        .expect("register");
    let status = invoke(&kernel, "latex.engine-status", Value::Null).expect("status");
    assert_eq!(status["downloadable"], false);
    assert_eq!(status["reason"], "engine_not_published");
    let err =
        invoke(&kernel, "latex.engine-install", json!({ "consent": true })).expect_err("cannot");
    assert!(err.contains("[engine_not_published]"), "{err}");
}

/// The real Tectonic worker through the real installer. Skipped unless
/// `MANANG_REAL_WORKER` names a built `manang-latex-worker` (see
/// `docs/ops/engine-release.md`); `MANANG_LATEX_CACHE` may point at a warm
/// package cache so no network is needed.
#[test]
fn the_real_worker_installs_and_compiles_the_corpus_article() {
    let Some(worker) = std::env::var_os("MANANG_REAL_WORKER") else {
        eprintln!("SKIP: MANANG_REAL_WORKER is not set");
        return;
    };
    let bytes = std::fs::read(&worker).expect("read the worker");
    let server = serve(bytes.clone(), Mode::Normal);
    let dir = tempfile::tempdir().expect("tmp");
    let cache = std::env::var_os("MANANG_LATEX_CACHE")
        .map_or_else(|| dir.path().join("cache"), std::path::PathBuf::from);
    let config = LatexConfig {
        engines_dir: Some(dir.path().join("engines")),
        engine_source: Some(source_for(&server, &bytes)),
        cache_dir: Some(cache),
        ..LatexConfig::default()
    };
    let plugin = LatexPlugin::with_tools(config, TexTools::default());
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest"), Arc::new(plugin))
        .expect("register");
    invoke(&kernel, "latex.engine-install", json!({ "consent": true })).expect("installs");
    let main = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/corpus/latex-article/main.tex"
    );
    let report = invoke(
        &kernel,
        "latex.compile",
        json!({ "main": main, "allow_network": true }),
    )
    .expect("compiles");
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["backend"], "tectonic");
    assert_eq!(report["tex_version"], "Tectonic 9.9.9");
}
