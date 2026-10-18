//! The SDK end to end: build the template and the word-count plugin with
//! `scripts/build-plugin.sh`, install them, and drive them through the kernel.
//!
//! Needs the `wasm32-unknown-unknown` target.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use manang_kernel::{Hook, Kernel, Payload};
use manang_plugins::{LoadError, ManagerConfig, PluginManager};
use manang_wasm_host::{BufferService, HostServices, WasmError};
use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Build a plugin crate into a directory shared by this test process.
fn build(crate_dir: &str) -> PathBuf {
    static LOCK: Mutex<()> = Mutex::new(());
    static OUT: OnceLock<PathBuf> = OnceLock::new();
    let out = OUT.get_or_init(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("sdk-dist"));
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let status = Command::new("bash")
        .arg(repo_root().join("scripts/build-plugin.sh"))
        .arg(repo_root().join(crate_dir))
        .arg(out)
        .status()
        .expect("bash runs");
    assert!(status.success(), "build-plugin.sh {crate_dir} failed");
    out.clone()
}

fn install(dist: &Path, name: &str, plugins: &Path) {
    let from = dist.join(name);
    let to = plugins.join(name);
    std::fs::create_dir_all(&to).expect("mkdir");
    for file in ["manifest.toml", "plugin.wasm"] {
        std::fs::copy(from.join(file), to.join(file)).expect("copy");
    }
}

struct Doc(&'static str);

impl BufferService for Doc {
    fn read_active(&self) -> Result<String, String> {
        Ok(self.0.to_owned())
    }
    fn write_active(&self, _: &str) -> Result<(), String> {
        Err("read-only".to_owned())
    }
}

fn manager(plugins: &Path, doc: &'static str) -> (Arc<Kernel>, PluginManager) {
    let kernel = Arc::new(Kernel::new());
    let mut config = ManagerConfig::new(plugins);
    config.services = HostServices {
        buffer: Some(Arc::new(Doc(doc))),
        ..HostServices::default()
    };
    let manager = PluginManager::new(Arc::clone(&kernel), config).expect("manager");
    (kernel, manager)
}

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected json, got {other:?}"),
    }
}

#[test]
fn word_count_counts_text_the_buffer_and_lints() {
    let dist = build("plugins/word-count");
    let tmp = tempfile::tempdir().expect("tempdir");
    install(&dist, "word-count", tmp.path());
    let (kernel, manager) = manager(tmp.path(), "one two three\nfour\n");
    let report = manager.scan().expect("scan");
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    // It asks for the active buffer, so it waits for the user's approval.
    assert_eq!(report.held, ["word-count"]);
    let asked = manager.state_of("word-count").expect("state").permissions;
    manager.approve("word-count", &asked).expect("approve");

    // Text passed as a JSON string, and as {"text": ...}.
    let expected = json!({"words": 5, "characters": 30, "lines": 2});
    for input in [
        json!("Hello, world!\nA second  line.\n"),
        json!({"text": "Hello, world!\nA second  line.\n"}),
    ] {
        let out = kernel
            .invoke("word-count.count", Payload::Json(input))
            .expect("counts");
        assert_eq!(json_of(out), expected);
    }
    // No text: the active buffer.
    let out = kernel
        .invoke("word-count.count", Payload::Empty)
        .expect("counts the buffer");
    assert_eq!(
        json_of(out),
        json!({"words": 4, "characters": 19, "lines": 2})
    );
    // Unicode: scalar values, not bytes.
    let out = kernel
        .invoke("word-count.count", Payload::Json(json!("naïve café")))
        .expect("counts");
    assert_eq!(json_of(out)["characters"], json!(10));
    // Bad input is an error, not a crash.
    assert!(
        kernel
            .invoke("word-count.count", Payload::Json(json!(42)))
            .is_err()
    );

    // The on_lint subscription.
    let results = kernel.run_hook_each(Hook::OnLint, Payload::Json(json!("a b c")));
    assert_eq!(results.len(), 1);
    let lint = json_of(results.into_iter().next().expect("one").1.expect("lint ok"));
    assert_eq!(lint["plugin"], json!("word-count"));
    assert_eq!(lint["diagnostics"][0]["severity"], json!("info"));
    assert!(
        lint["diagnostics"][0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("3 words"))
    );
}

#[test]
fn word_count_needs_its_buffer_permission() {
    let dist = build("plugins/word-count");
    let tmp = tempfile::tempdir().expect("tempdir");
    install(&dist, "word-count", tmp.path());
    // Strip the permission: the component still imports the buffer, so the
    // host refuses to load it rather than failing later.
    std::fs::write(
        tmp.path().join("word-count/manifest.toml"),
        "[plugin]\nname = \"word-count\"\nversion = \"0.1.0\"\n\
         [[contributes.commands]]\nid = \"word-count.count\"\ntitle = \"Count\"\n",
    )
    .expect("write");
    let (_kernel, manager) = manager(tmp.path(), "");
    let report = manager.scan().expect("scan");
    assert!(report.loaded.is_empty());
    assert!(matches!(
        report.failed[0].error,
        LoadError::Wasm(WasmError::UngrantedImport { .. })
    ));
}

#[test]
fn the_template_builds_and_runs() {
    let dist = build("sdk/rust-plugin-template");
    let tmp = tempfile::tempdir().expect("tempdir");
    install(&dist, "my-plugin", tmp.path());
    let (kernel, manager) = manager(tmp.path(), "");
    assert!(manager.scan().expect("scan").failed.is_empty());
    let out = kernel
        .invoke("my-plugin.hello", Payload::Json(json!("Manang")))
        .expect("hello");
    assert_eq!(json_of(out), json!({"greeting": "Hello, Manang!"}));
    let out = kernel
        .invoke("my-plugin.hello", Payload::Empty)
        .expect("hello");
    assert_eq!(json_of(out), json!({"greeting": "Hello, world!"}));
}

/// The three copies of the WIT (host, template, word-count) must not drift.
#[test]
fn bundled_wit_copies_match_the_host_wit() {
    let host = std::fs::read(repo_root().join("crates/manang-wasm-host/wit/plugin.wit"))
        .expect("host wit");
    for copy in [
        "sdk/rust-plugin-template/wit/plugin.wit",
        "plugins/word-count/wit/plugin.wit",
    ] {
        let bytes = std::fs::read(repo_root().join(copy)).expect("copy exists");
        assert_eq!(bytes, host, "{copy} is out of sync; re-copy the host WIT");
    }
}
