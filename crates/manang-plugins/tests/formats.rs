//! Golden-fixture migration tests for `plugins.toml` (user decisions) and the
//! project's `.manang/plugins.toml` override (`docs/formats.md`).
//!
//! `v1/` is the file as the first release wrote it (no `version` key). `v999/`
//! is a made-up future version: read as far as it is understood, never
//! rewritten, nothing lost.

use std::fs;
use std::path::{Path, PathBuf};

use manang_plugins::registry::{ProjectOverride, Registry, RegistryError};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/formats")
        .join(rel)
}

fn copy_to(rel: &str, dir: &Path, name: &str) -> PathBuf {
    let target = dir.join(name);
    fs::create_dir_all(dir).expect("mkdir");
    fs::copy(fixture(rel), &target).expect("copy fixture");
    target
}

#[test]
fn plugins_toml_v1_loads_and_is_stamped_on_the_next_save_without_loss() {
    let tmp = tempfile::tempdir().expect("tmp");
    let file = copy_to("plugins-toml/v1/plugins.toml", tmp.path(), "plugins.toml");
    let mut registry = Registry::load(&file).expect("v1 loads");
    assert_eq!(registry.newer_version(), None);
    let names: Vec<_> = registry.names().cloned().collect();
    assert_eq!(names, ["slide-themes", "word-count"]);
    insta::assert_debug_snapshot!("plugins_v1_loaded", registry.get("word-count"));

    registry.entry("word-count").enabled = false;
    registry.save().expect("save");
    let text = fs::read_to_string(&file).expect("read");
    assert!(text.contains("\nversion = 1\n"), "{text}");
    insta::assert_snapshot!("plugins_v1_saved", text);
    let again = Registry::load(&file).expect("reload");
    assert_eq!(again.get("slide-themes"), registry.get("slide-themes"));
    assert!(!again.get("word-count").expect("kept").enabled);
    assert_eq!(
        again.get("word-count").expect("kept").granted.network,
        ["api.example.org"],
        "approved permissions survive"
    );
}

#[test]
fn plugins_toml_v999_is_read_but_never_written() {
    let tmp = tempfile::tempdir().expect("tmp");
    let file = copy_to("plugins-toml/v999/plugins.toml", tmp.path(), "plugins.toml");
    let original = fs::read_to_string(&file).expect("read");
    let mut registry = Registry::load(&file).expect("a newer file is not an error");
    assert_eq!(registry.newer_version(), Some(999));
    assert_eq!(
        registry
            .get("word-count")
            .and_then(|r| r.approved_version.clone()),
        Some("0.3.0".to_owned()),
        "what is understood is kept"
    );
    registry.entry("word-count").enabled = false;
    let err = registry.save().expect_err("read-only");
    assert!(matches!(err, RegistryError::Format(_)), "{err}");
    assert!(err.to_string().contains("newer Manang"), "{err}");
    assert_eq!(
        fs::read_to_string(&file).expect("read"),
        original,
        "`trust`, `[telemetry]` and every other future key are still on disk"
    );
}

#[test]
fn project_override_v1_and_v999_both_load_and_only_ever_disable() {
    let tmp = tempfile::tempdir().expect("tmp");
    let manang = tmp.path().join(".manang");
    copy_to(
        "project-plugins-toml/v1/plugins.toml",
        &manang,
        "plugins.toml",
    );
    let v1 = ProjectOverride::load(tmp.path()).expect("v1");
    assert_eq!(v1.disabled, ["git", "search"]);
    assert_eq!(v1.version, None);

    let file = copy_to(
        "project-plugins-toml/v999/plugins.toml",
        &manang,
        "plugins.toml",
    );
    let before = fs::read(&file).expect("read");
    let v999 = ProjectOverride::load(tmp.path()).expect("a newer override is read leniently");
    assert_eq!(v999.disabled, ["git"]);
    assert_eq!(v999.version, Some(999));
    assert_eq!(fs::read(&file).expect("read"), before, "never written");
}
