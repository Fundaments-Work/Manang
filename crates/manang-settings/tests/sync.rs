//! Kernel-driven tests for plugin-declared settings and `<plugin>.configure`.

use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_kernel::{Context, EventBus, Kernel, Manifest, Payload, Plugin, PluginError};
use manang_settings::{PluginSettingsSync, SettingsPlugin, SyncError, manifests};
use parking_lot::Mutex;
use serde_json::{Value, json};

const DEMO: &str = r#"
[plugin]
name = "demo"
version = "0.1.0"
tier = "native"
settings_contexts = ["document", "engine:demo"]

[[contributes.commands]]
id = "demo.configure"
title = "Apply demo settings"

[[contributes.settings]]
id = "demo.mode"
type = "enum"
values = ["fast", "slow"]
default = "fast"
title = "Mode"
description = "How the demo runs."

[[contributes.settings]]
id = "demo.level"
type = "int"
min = 0
max = 10
default = 3
scope = "project"
description = "Demo level."

[[contributes.settings]]
id = "demo.globs"
type = "list"
item = { type = "string" }
default = ["*.tmp"]
description = "Demo globs."
"#;

/// Declares a setting but provides no `configure` command.
const QUIET: &str = r#"
[plugin]
name = "quiet"
version = "0.1.0"

[[contributes.settings]]
id = "quiet.flag"
type = "bool"
default = false
description = "A flag nobody receives."
"#;

#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<Value>>,
}

impl Recorder {
    fn last(&self) -> Option<Value> {
        self.calls.lock().last().cloned()
    }

    fn count(&self) -> usize {
        self.calls.lock().len()
    }
}

impl Plugin for Recorder {
    fn invoke(
        &self,
        _: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match (command, input) {
            ("demo.configure", Payload::Json(v)) => {
                self.calls.lock().push(v["values"].clone());
                Ok(Payload::Empty)
            }
            (other, _) => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}

struct Rig {
    kernel: Arc<Kernel>,
    bus: EventBus,
    demo: Arc<Recorder>,
    _config: tempfile::TempDir,
}

fn rig() -> Rig {
    let config = tempfile::tempdir().expect("tmp");
    let bus = EventBus::new();
    let kernel = Arc::new(Kernel::new());
    let settings = Arc::new(SettingsPlugin::new(config.path(), bus.clone()));
    for m in manifests().expect("manifests") {
        kernel
            .register(m, settings.clone())
            .expect("register settings");
    }
    let demo = Arc::new(Recorder::default());
    kernel
        .register(Manifest::from_toml(DEMO).expect("demo"), demo.clone())
        .expect("register demo");
    kernel
        .register(
            Manifest::from_toml(QUIET).expect("quiet"),
            Arc::new(Recorder::default()),
        )
        .expect("register quiet");
    Rig {
        kernel,
        bus,
        demo,
        _config: config,
    }
}

impl Rig {
    fn ok(&self, command: &str, args: Value) -> Value {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => v,
            other => panic!("{command}: {other:?}"),
        }
    }
}

/// Wait until `demo.configure` has been called `n` times.
fn wait_calls(rec: &Recorder, n: usize) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if rec.count() >= n {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn sync_registers_manifest_settings_and_pushes_defaults() {
    let rig = rig();
    let sync = PluginSettingsSync::new(&rig.kernel);
    let outcome = sync.sync_plugin("demo").expect("sync demo");
    assert_eq!(outcome.declared, 3);
    assert!(outcome.pushed);
    assert_eq!(
        rig.demo.last(),
        Some(json!({"demo.mode": "fast", "demo.level": 3, "demo.globs": ["*.tmp"]}))
    );

    let schema = rig.ok("settings.schema", json!({}));
    let settings = schema["settings"].as_array().expect("settings");
    let mode = settings
        .iter()
        .find(|d| d["id"] == "demo.mode")
        .expect("demo.mode registered");
    assert_eq!(
        *mode,
        json!({
            "id": "demo.mode",
            "type": "enum",
            "values": ["fast", "slow"],
            "default": "fast",
            "description": "How the demo runs.",
            "scope": "user",
            "requires-restart": false,
            "title": "Mode",
            "owner": "demo",
            "category": "demo",
            "contexts": ["document", "engine:demo"],
        })
    );
    let level = settings
        .iter()
        .find(|d| d["id"] == "demo.level")
        .expect("demo.level");
    assert_eq!(level["scope"], "project");
    assert!(level.get("title").is_none(), "no title is omitted");

    // Idempotent: a second sync registers nothing new and pushes again.
    assert!(sync.sync_plugin("demo").expect("again").pushed);
    assert_eq!(rig.demo.count(), 2);
}

#[test]
fn plugins_without_configure_or_settings_are_registered_but_not_pushed() {
    let rig = rig();
    let sync = PluginSettingsSync::new(&rig.kernel);
    let quiet = sync.sync_plugin("quiet").expect("quiet");
    assert_eq!((quiet.declared, quiet.pushed), (1, false));
    let got = rig.ok("settings.get", json!({"id": "quiet.flag"}));
    assert_eq!(got["value"], false);

    let none = sync.sync_plugin("settings").expect("settings itself");
    assert_eq!((none.declared, none.pushed), (0, false));
    assert!(matches!(
        sync.sync_plugin("missing"),
        Err(SyncError::UnknownPlugin(_))
    ));

    let all = sync.sync_all();
    assert!(all.iter().all(|(_, r)| r.is_ok()), "{all:?}");
    assert_eq!(all.len(), rig.kernel.plugin_names().len());
}

#[test]
fn listener_pushes_changes_and_project_switches() {
    let rig = rig();
    let sync = PluginSettingsSync::new(&rig.kernel);
    sync.sync_plugin("demo").expect("sync");
    sync.start(&rig.bus).expect("start");
    assert!(sync.is_running());
    let mut seen = rig.demo.count();

    rig.ok("settings.set", json!({"id": "demo.mode", "value": "slow"}));
    assert!(wait_calls(&rig.demo, seen + 1), "change was pushed");
    assert_eq!(rig.demo.last().expect("values")["demo.mode"], "slow");
    seen = rig.demo.count();

    // A core setting change does not reach demo.
    rig.ok("settings.set", json!({"id": "editor.tab-size", "value": 4}));

    // Switching to a project that overrides demo.level pushes it.
    let project = tempfile::tempdir().expect("project");
    fs::create_dir_all(project.path().join(".manang")).expect("mkdir");
    fs::write(
        project.path().join(".manang/settings.toml"),
        "[demo]\nlevel = 9\n",
    )
    .expect("write");
    rig.ok(
        "settings.use-project",
        json!({"path": project.path().to_string_lossy()}),
    );
    assert!(wait_calls(&rig.demo, seen + 1), "project switch was pushed");
    let deadline = Instant::now() + Duration::from_secs(5);
    while rig.demo.last().expect("values")["demo.level"] != 9 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let values = rig.demo.last().expect("values");
    assert_eq!(values["demo.level"], 9);
    assert_eq!(values["demo.mode"], "slow");

    // Stopping ends the thread; later changes are not pushed.
    sync.stop();
    assert!(!sync.is_running());
    let stopped_at = rig.demo.count();
    rig.ok("settings.set", json!({"id": "demo.mode", "value": "fast"}));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(rig.demo.count(), stopped_at);
}

#[test]
fn listener_exits_when_the_kernel_is_dropped() {
    let rig = rig();
    let sync = PluginSettingsSync::new(&rig.kernel);
    sync.start(&rig.bus).expect("start");
    let Rig { kernel, bus, .. } = rig;
    drop(kernel);
    let deadline = Instant::now() + Duration::from_secs(5);
    while sync.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!sync.is_running());
    assert!(matches!(
        sync.sync_plugin("demo"),
        Err(SyncError::KernelGone)
    ));
    drop(bus);
}

#[test]
fn a_reloaded_manifest_replaces_its_definitions() {
    let rig = rig();
    let sync = PluginSettingsSync::new(&rig.kernel);
    sync.sync_plugin("demo").expect("sync");
    let changed = DEMO.replace("default = 3", "default = 5");
    rig.kernel
        .replace(
            Manifest::from_toml(&changed).expect("changed"),
            rig.demo.clone(),
        )
        .expect("replace");
    sync.sync_plugin("demo").expect("resync");
    assert_eq!(rig.demo.last().expect("values")["demo.level"], 5);
}
