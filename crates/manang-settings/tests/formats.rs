//! Golden-fixture migration tests for the files this crate owns: `settings.toml`,
//! the project `.manang/settings.toml` and `keybindings.toml` (`docs/formats.md`).
//!
//! `testdata/formats/<format>/v1/` is the file as the first release wrote it (no
//! `version` key). `v999/` is a made-up future version: read as far as it is
//! understood, never rewritten, nothing lost.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{EventBus, Kernel, KernelError, Payload};
use manang_settings::{SettingsPlugin, manifests};
use serde_json::{Value, json};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/formats")
        .join(rel)
}

struct Rig {
    kernel: Kernel,
    config: tempfile::TempDir,
}

/// A config dir seeded with fixtures: `(fixture path, file name in the config dir)`.
fn rig(seeds: &[(&str, &str)]) -> Rig {
    rig_with(seeds, &[])
}

/// Like [`rig`], plus files written from text before the plugin starts.
fn rig_with(seeds: &[(&str, &str)], texts: &[(&str, &str)]) -> Rig {
    let config = tempfile::tempdir().expect("tmp");
    for (rel, name) in seeds {
        fs::copy(fixture(rel), config.path().join(name)).expect("seed");
    }
    for (name, text) in texts {
        fs::write(config.path().join(name), text).expect("seed text");
    }
    let plugin = Arc::new(
        SettingsPlugin::new(config.path(), EventBus::new())
            .with_debounce(Duration::from_millis(30)),
    );
    let kernel = Kernel::new();
    for m in manifests().expect("manifests") {
        kernel.register(m, plugin.clone()).expect("register");
    }
    Rig { kernel, config }
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

    fn file(&self, name: &str) -> String {
        fs::read_to_string(self.config.path().join(name)).expect("read")
    }
}

fn newer_warning(warnings: &Value) -> bool {
    warnings
        .as_array()
        .is_some_and(|w| w.iter().any(|w| w.to_string().contains("newer Manang")))
}

#[test]
fn settings_v1_is_read_then_stamped_on_write_keeping_comments_and_unknown_keys() {
    let rig = rig(&[("settings-toml/v1/settings.toml", "settings.toml")]);
    let all = rig.ok("settings.get", json!({}));
    assert_eq!(all["values"]["editor.font-size"]["value"], 15);
    assert_eq!(all["values"]["editor.tab-size"]["value"], 4);
    assert!(
        !newer_warning(&all["warnings"]),
        "an unversioned file is simply v1"
    );
    rig.ok("settings.set", json!({"id": "editor.tab-size", "value": 2}));
    let text = rig.file("settings.toml");
    insta::assert_snapshot!("settings_v1_after_write", text);
    assert!(text.contains("[zzz]\nunknown-here = true"), "{text}");
    assert!(text.contains("# the font size I like"), "{text}");
    // `version` is the file's, never offered as a setting.
    let all = rig.ok("settings.get", json!({}));
    assert!(all["values"].get("version").is_none());
    assert!(
        !all["warnings"]
            .to_string()
            .contains("unknown setting `version`"),
        "{}",
        all["warnings"]
    );
}

#[test]
fn settings_v999_is_read_but_never_written() {
    let rig = rig(&[("settings-toml/v999/settings.toml", "settings.toml")]);
    let original = rig.file("settings.toml");
    let all = rig.ok("settings.get", json!({}));
    assert_eq!(all["values"]["editor.font-size"]["value"], 17);
    assert!(newer_warning(&all["warnings"]), "{}", all["warnings"]);
    let err = rig
        .call("settings.set", json!({"id": "editor.tab-size", "value": 2}))
        .expect_err("refused");
    assert!(err.contains("newer Manang"), "{err}");
    let err = rig
        .call("settings.reset", json!({"id": "editor.font-size"}))
        .expect_err("refused");
    assert!(err.contains("newer Manang"), "{err}");
    assert_eq!(rig.file("settings.toml"), original, "not a byte changed");
}

#[test]
fn project_settings_v1_loads_and_v999_is_left_alone() {
    let rig = rig(&[]);
    let project = tempfile::tempdir().expect("project");
    let dir = project.path().join(".manang");
    fs::create_dir_all(&dir).expect("mkdir");
    fs::copy(
        fixture("project-settings-toml/v1/settings.toml"),
        dir.join("settings.toml"),
    )
    .expect("seed");
    rig.ok(
        "settings.use-project",
        json!({"path": project.path().to_string_lossy()}),
    );
    let got = rig.ok("settings.get", json!({"id": "editor.tab-size"}));
    assert_eq!(
        (got["value"].clone(), got["source"].clone()),
        (json!(8), json!("project"))
    );
    // A future file: replace the fixture and reload.
    fs::write(
        dir.join("settings.toml"),
        "version = 999\n[editor]\ntab-size = 3\nfuture = 1\n",
    )
    .expect("future");
    rig.ok(
        "settings.use-project",
        json!({"path": project.path().to_string_lossy()}),
    );
    let before = fs::read_to_string(dir.join("settings.toml")).expect("read");
    let err = rig
        .call(
            "settings.set",
            json!({"id": "editor.tab-size", "value": 2, "scope": "project"}),
        )
        .expect_err("refused");
    assert!(err.contains("newer Manang"), "{err}");
    assert_eq!(
        fs::read_to_string(dir.join("settings.toml")).expect("read"),
        before
    );
}

#[test]
fn keybindings_v1_loads_and_is_stamped_on_write_keeping_the_rest() {
    let rig = rig(&[("keybindings-toml/v1/keybindings.toml", "keybindings.toml")]);
    let listed = rig.ok("keybindings.list", json!({}));
    assert!(listed.to_string().contains("Mod+Alt+S"));
    rig.ok(
        "keybindings.set",
        json!({"command": "a.other", "keys": ["Mod+K"]}),
    );
    let text = rig.file("keybindings.toml");
    insta::assert_snapshot!("keybindings_v1_after_write", text);
    assert!(
        text.contains("# Mine") && text.contains("Mod+Alt+S"),
        "{text}"
    );
}

#[test]
fn keybindings_v999_is_never_rewritten() {
    let rig = rig(&[("keybindings-toml/v999/keybindings.toml", "keybindings.toml")]);
    let original = rig.file("keybindings.toml");
    let listed = rig.ok("keybindings.list", json!({}));
    assert!(listed.to_string().contains("Mod+Alt+S"));
    assert!(
        listed.to_string().contains("newer Manang"),
        "the user is told: {listed}"
    );
    let err = rig
        .call(
            "keybindings.set",
            json!({"command": "a.other", "keys": ["Mod+K"]}),
        )
        .expect_err("refused");
    assert!(err.contains("newer Manang"), "{err}");
    let err = rig
        .call("keybindings.reset", json!({}))
        .expect_err("refused");
    assert!(err.contains("newer Manang"), "{err}");
    assert_eq!(rig.file("keybindings.toml"), original);
}

#[test]
fn a_damaged_settings_file_is_reported_and_never_overwritten() {
    // Hand-edited files are not set aside (the person may be mid-edit): they
    // are left exactly as they are, the app runs on defaults and says why.
    let rig = rig_with(&[], &[("settings.toml", "editor = = oops")]);
    let all = rig.ok("settings.get", json!({}));
    assert!(all["warnings"].to_string().contains("not valid TOML"));
    assert!(
        rig.call("settings.set", json!({"id": "editor.tab-size", "value": 4}))
            .is_err()
    );
    assert_eq!(rig.file("settings.toml"), "editor = = oops");
}
