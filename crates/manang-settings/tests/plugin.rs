//! Kernel-driven integration tests on temp directories.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{EventBus, Kernel, KernelError, Payload, Subscription};
use manang_settings::{SettingsPlugin, manifests};
use serde_json::{Value, json};

struct Rig {
    kernel: Kernel,
    bus: EventBus,
    config: tempfile::TempDir,
}

fn rig() -> Rig {
    rig_seeded(None)
}

fn rig_seeded(seed: Option<&str>) -> Rig {
    let config = tempfile::tempdir().expect("tmp");
    if let Some(text) = seed {
        fs::write(config.path().join("settings.toml"), text).expect("seed");
    }
    let bus = EventBus::new();
    let plugin =
        SettingsPlugin::new(config.path(), bus.clone()).with_debounce(Duration::from_millis(30));
    let kernel = Kernel::new();
    let plugin = Arc::new(plugin);
    for m in manifests().expect("manifests") {
        kernel.register(m, plugin.clone()).expect("register");
    }
    Rig {
        kernel,
        bus,
        config,
    }
}

impl Rig {
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(other) => Err(format!("unexpected payload {other:?}")),
            Err(KernelError::Plugin(e)) => Err(e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        self.call(command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    fn user_file(&self) -> std::path::PathBuf {
        self.config.path().join("settings.toml")
    }

    fn project(&self) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("project");
        self.ok(
            "settings.use-project",
            json!({"path": dir.path().to_string_lossy()}),
        );
        dir
    }
}

fn topics(sub: &Subscription) -> Vec<(String, Value)> {
    sub.drain()
        .into_iter()
        .map(|e| match e.payload {
            Payload::Json(v) => (e.topic, v),
            _ => (e.topic, Value::Null),
        })
        .collect()
}

fn wait_for(sub: &Subscription, topic: &str, id: &str) -> Option<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let event = sub.recv_timeout(left)?;
        if let Payload::Json(v) = event.payload
            && event.topic == topic
            && v["id"] == id
        {
            return Some(v);
        }
    }
    None
}

#[test]
fn every_manifest_command_is_handled() {
    let rig = rig();
    for command in rig.kernel.commands() {
        let result = rig.call(&command.id, json!({"__probe": true}));
        // Bad arguments are fine; "does not handle" is not.
        if let Err(message) = result {
            assert!(!message.contains("does not handle"), "{}", command.id);
        }
    }
}

#[test]
fn get_set_reset_round_trip_with_events() {
    let rig = rig();
    let sub = rig.bus.subscribe("settings.*");

    let got = rig.ok("settings.get", json!({"id": "editor.font-size"}));
    assert_eq!(got["value"], 14);
    assert_eq!(got["source"], "default");

    let set = rig.ok(
        "settings.set",
        json!({"id": "editor.font-size", "value": 18}),
    );
    assert_eq!(set["value"], 18);
    assert_eq!(set["source"], "user");
    let events = topics(&sub);
    assert_eq!(
        events,
        vec![(
            "settings.changed".to_owned(),
            json!({"id": "editor.font-size", "value": 18, "scope": "user", "requires-restart": false})
        )]
    );
    assert!(
        fs::read_to_string(rig.user_file())
            .expect("file")
            .contains("font-size = 18")
    );

    // Setting the same value again changes nothing.
    rig.ok(
        "settings.set",
        json!({"id": "editor.font-size", "value": 18}),
    );
    assert!(topics(&sub).is_empty());

    let reset = rig.ok("settings.reset", json!({"id": "editor.font-size"}));
    assert_eq!(reset["value"], 14);
    let events = topics(&sub);
    assert_eq!(events[0].1["scope"], "default");
    assert!(
        !fs::read_to_string(rig.user_file())
            .expect("file")
            .contains("font-size")
    );
}

#[test]
fn invalid_writes_are_rejected_precisely() {
    let rig = rig();
    let cases = [
        json!({"id": "editor.font-size", "value": 500}),
        json!({"id": "editor.font-size", "value": "big"}),
        json!({"id": "appearance.theme", "value": "sepia"}),
        json!({"id": "nope.nothing", "value": 1}),
        json!({"id": "files.autosave", "value": "off", "scope": "project"}),
        json!({"id": "editor.tab-size", "value": 4, "scope": "project"}),
    ];
    let errors: Vec<String> = cases
        .into_iter()
        .map(|c| rig.call("settings.set", c).expect_err("rejected"))
        .collect();
    insta::assert_debug_snapshot!(errors);
    assert!(
        !rig.user_file().exists(),
        "a rejected write must not create the file"
    );
}

#[test]
fn layers_stack_and_project_scope_is_enforced() {
    let rig = rig();
    let sub = rig.bus.subscribe("settings.changed");
    rig.ok("settings.set", json!({"id": "editor.tab-size", "value": 4}));
    let project = rig.project();
    topics(&sub);

    rig.ok(
        "settings.set",
        json!({"id": "editor.tab-size", "value": 8, "scope": "project"}),
    );
    let got = rig.ok("settings.get", json!({"id": "editor.tab-size"}));
    assert_eq!(
        (got["value"].clone(), got["source"].clone()),
        (json!(8), json!("project"))
    );
    let events = topics(&sub);
    assert_eq!(events[0].1["scope"], "project");
    let written = fs::read_to_string(project.path().join(".manang/settings.toml")).expect("file");
    assert_eq!(written, "[editor]\ntab-size = 8\n");

    // Leaving the project falls back to the user value and says so.
    rig.ok("settings.use-project", json!({"path": null}));
    let got = rig.ok("settings.get", json!({"id": "editor.tab-size"}));
    assert_eq!(
        (got["value"].clone(), got["source"].clone()),
        (json!(4), json!("user"))
    );
    assert_eq!(topics(&sub)[0].1["value"], 4);

    let err = rig
        .call(
            "settings.set",
            json!({"id": "editor.tab-size", "value": 2, "scope": "project"}),
        )
        .expect_err("no project");
    assert!(err.contains("no project is selected"), "{err}");
}

#[test]
fn comments_survive_and_unknown_keys_are_kept_and_reported() {
    let seed = "# my notes\n[editor]\nfont-size = 12 # small\nmystery = true\n\n[zzz]\nflag = 1\n";
    let rig = rig_seeded(Some(seed));
    let all = rig.ok("settings.get", json!({}));
    assert_eq!(all["values"]["editor.font-size"]["value"], 12);
    insta::assert_json_snapshot!(all["warnings"]);
    rig.ok(
        "settings.set",
        json!({"id": "editor.font-size", "value": 16}),
    );
    assert_eq!(
        fs::read_to_string(rig.user_file()).expect("file"),
        seed.replace("12", "16")
    );
}

#[test]
fn a_broken_file_is_never_overwritten() {
    let rig = rig();
    fs::write(rig.user_file(), "editor = = oops").expect("seed");
    let err = rig
        .call("settings.set", json!({"id": "editor.tab-size", "value": 4}))
        .expect_err("refused");
    assert!(err.contains("not valid TOML"), "{err}");
    assert_eq!(
        fs::read_to_string(rig.user_file()).expect("file"),
        "editor = = oops"
    );
}

#[test]
fn plugins_register_settings_over_the_command() {
    let rig = rig();
    let sub = rig.bus.subscribe("settings.*");
    let def = json!({
        "id": "latex-tools.shell-escape", "type": "enum", "values": ["off", "restricted"],
        "default": "off", "description": "Shell escape policy.", "requires-restart": true
    });
    let out = rig.ok(
        "settings.register",
        json!({"settings": [def.clone()], "owner": "latex-tools"}),
    );
    assert_eq!(out["registered"], 1);
    assert_eq!(
        rig.ok(
            "settings.register",
            json!({"settings": [def], "owner": "latex-tools"})
        )["registered"],
        0
    );
    assert_eq!(topics(&sub).len(), 1);

    let set = rig.ok(
        "settings.set",
        json!({"id": "latex-tools.shell-escape", "value": "restricted"}),
    );
    assert_eq!(set["requires-restart"], true);
    let events = topics(&sub);
    assert_eq!(events[0].1["requires-restart"], true);

    let schema = rig.ok("settings.schema", json!({}));
    let ids: Vec<&str> = schema["settings"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert!(ids.contains(&"latex-tools.shell-escape") && ids.contains(&"editor.font-size"));

    // Core ids are protected and bad owners refused.
    let hijack =
        json!({"id": "editor.tab-size", "type": "bool", "default": true, "description": "x"});
    assert!(
        rig.call(
            "settings.register",
            json!({"settings": [hijack], "owner": "evil"})
        )
        .is_err()
    );
    assert!(
        rig.call(
            "settings.register",
            json!({"settings": [], "owner": "core"})
        )
        .is_err()
    );
}

#[test]
fn open_file_returns_and_creates_the_path() {
    let rig = rig();
    let out = rig.ok("settings.open-file", json!({"scope": "user"}));
    assert_eq!(
        Path::new(out["path"].as_str().expect("path")),
        rig.user_file()
    );
    assert!(rig.user_file().exists());
    // The created file is valid and does not disturb values.
    assert_eq!(
        rig.ok("settings.get", json!({"id": "editor.tab-size"}))["value"],
        2
    );
    assert!(
        rig.call("settings.open-file", json!({"scope": "project"}))
            .is_err()
    );
    let keys = rig.ok("settings.open-file", json!({"scope": "keybindings"}));
    assert!(
        keys["path"]
            .as_str()
            .expect("path")
            .ends_with("keybindings.toml")
    );
}

#[test]
fn keybinding_commands_end_to_end() {
    let rig = rig();
    let sub = rig.bus.subscribe("keybindings.*");

    let listed = rig.ok("keybindings.list", json!({}));
    assert_eq!(listed["conflicts"], json!([]));
    assert!(listed["bindings"].as_array().expect("array").len() >= 10);

    let hit = rig.ok(
        "keybindings.resolve",
        json!({"context": ["editorFocus"], "key": "ctrl+s"}),
    );
    let hit_mac = rig.ok(
        "keybindings.resolve",
        json!({"context": "editorFocus", "key": "Mod+S"}),
    );
    assert_eq!(hit["status"], hit_mac["status"]);

    // Rebind onto a key another command owns: allowed, but reported.
    let set = rig.ok(
        "keybindings.set",
        json!({"command": "doc.compile", "keys": ["mod+s"]}),
    );
    assert_eq!(set["keys"], json!(["Mod+S"]));
    assert_eq!(set["conflicts"][0]["kind"], "shadowed");
    assert_eq!(topics(&sub).len(), 1);
    let text = fs::read_to_string(rig.config.path().join("keybindings.toml")).expect("file");
    assert!(text.contains("command = \"doc.compile\""), "{text}");

    // Two user bindings on one key tie: resolve reports it, never picks.
    rig.ok(
        "keybindings.set",
        json!({"command": "a.other", "key": "Mod+S"}),
    );
    let tied = rig.ok(
        "keybindings.resolve",
        json!({"context": [], "key": "Mod+S"}),
    );
    assert_eq!(tied["status"], "conflict");
    assert_eq!(tied["commands"], json!(["a.other", "doc.compile"]));

    rig.ok("keybindings.reset", json!({"command": "a.other"}));
    rig.ok("keybindings.reset", json!({}));
    let after = rig.ok("keybindings.list", json!({}));
    assert_eq!(after["conflicts"], json!([]));

    assert!(
        rig.call(
            "keybindings.set",
            json!({"command": "a.b", "key": "Ctrl+Nope"})
        )
        .is_err()
    );
    assert!(
        rig.call(
            "keybindings.set",
            json!({"command": "Bad", "key": "Ctrl+J"})
        )
        .is_err()
    );

    // Plugins add defaults.
    let reg = rig.ok(
        "keybindings.register",
        json!({"bindings": [{"command": "figure.undo", "key": "Mod+Z", "when": "figureFocus"}]}),
    );
    assert_eq!(reg["registered"], 1);
    let r = rig.ok(
        "keybindings.resolve",
        json!({"context": ["figureFocus"], "key": "Ctrl+Z"}),
    );
    assert_eq!(r["command"], "figure.undo");
}

#[test]
fn theme_commands() {
    let rig = rig();
    let themes_dir = rig.config.path().join("themes");
    fs::create_dir_all(&themes_dir).expect("dir");
    fs::write(
        themes_dir.join("soft.toml"),
        "name = \"Soft\"\nbase = \"dark\"\n[tokens]\n\"--mng-bg\" = \"#18181b\"\n",
    )
    .expect("theme");
    fs::write(
        themes_dir.join("unreadable.toml"),
        "base = \"dark\"\n[tokens]\n\"--mng-text\" = \"#222222\"\n",
    )
    .expect("theme");

    let list = rig.ok("theme.list", json!({}));
    let ids: Vec<&str> = list["themes"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["dark", "light", "soft"]);
    assert_eq!(list["warnings"].as_array().expect("array").len(), 1);

    let dark = rig.ok("theme.get", json!({"id": "dark"}));
    insta::assert_json_snapshot!("theme_dark", dark);
    let light = rig.ok("theme.get", json!({"id": "light"}));
    assert_eq!(
        light["tokens"]["--mng-accent"],
        "color-mix(in srgb, #c6f135 35%, #1c1c1c)"
    );
    let soft = rig.ok("theme.get", json!({"id": "soft"}));
    assert_eq!(soft["tokens"]["--mng-bg"], "#18181b");
    assert!(rig.call("theme.get", json!({"id": "unreadable"})).is_err());
    assert!(
        rig.call("theme.get", json!({"id": "../../etc/passwd"}))
            .is_err()
    );
    assert!(rig.call("theme.get", json!({"id": "missing"})).is_err());
}

#[test]
fn external_edits_reload_and_emit_events() {
    let rig = rig();
    let sub = rig.bus.subscribe("settings.*");

    fs::write(rig.user_file(), "[editor]\nfont-size = 20\n").expect("external edit");
    let ev = wait_for(&sub, "settings.changed", "editor.font-size").expect("reload event");
    assert_eq!(
        (ev["value"].clone(), ev["scope"].clone()),
        (json!(20), json!("user"))
    );
    assert_eq!(
        rig.ok("settings.get", json!({"id": "editor.font-size"}))["value"],
        20
    );

    // An invalid value is reported, ignored, and the default returns.
    fs::write(rig.user_file(), "[editor]\nfont-size = 9000\n").expect("external edit");
    let ev = wait_for(&sub, "settings.changed", "editor.font-size").expect("fallback event");
    assert_eq!(ev["scope"], "default");
    let all = rig.ok("settings.get", json!({}));
    assert!(
        all["warnings"][0]["message"]
            .as_str()
            .expect("msg")
            .contains("above the maximum")
    );

    // External keybinding edits too.
    let ksub = rig.bus.subscribe("keybindings.changed");
    fs::write(
        rig.config.path().join("keybindings.toml"),
        "[[bindings]]\ncommand = \"project.open\"\nkeys = [\"Mod+Shift+O\"]\n",
    )
    .expect("keys");
    assert!(ksub.recv_timeout(Duration::from_secs(10)).is_some());
    let hit = rig.ok("keybindings.resolve", json!({"key": "Ctrl+Shift+O"}));
    // Written under the old id, read under the new one.
    assert_eq!(hit["command"], "project.switch");
}

#[test]
fn external_project_edits_reload() {
    let rig = rig();
    let project = rig.project();
    rig.ok("settings.open-file", json!({"scope": "project"}));
    let sub = rig.bus.subscribe("settings.changed");
    // Let the watcher restart settle, then edit.
    std::thread::sleep(Duration::from_millis(200));
    fs::write(
        project.path().join(".manang/settings.toml"),
        "[preview]\ndebounce-ms = 750\n",
    )
    .expect("edit");
    let ev = wait_for(&sub, "settings.changed", "preview.debounce-ms").expect("event");
    assert_eq!(
        (ev["value"].clone(), ev["scope"].clone()),
        (json!(750), json!("project"))
    );
}
