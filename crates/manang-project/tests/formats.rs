//! Golden-fixture migration tests for the formats this crate owns: `manang.toml`,
//! `projects.toml` and `recent-projects.toml` (see `docs/formats.md`).
//!
//! `testdata/formats/<format>/v1/` holds the file exactly as the first release
//! wrote it (no `version` key). `v999/` is a made-up future version: it must be
//! readable as far as it is understood, never rewritten, and lose nothing.

use std::fs;
use std::path::{Path, PathBuf};

use manang_project::ProjectConfig;
use manang_project::recent::{RECENT_FILE, RecentStore};
use manang_project::registry::{Patch, REGISTRY_FILE, Registry};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/formats")
        .join(rel)
}

/// A config dir holding a copy of one fixture under its real file name.
fn config_with(rel: &str, name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tmp");
    fs::copy(fixture(rel), dir.path().join(name)).expect("copy fixture");
    dir
}

fn read(dir: &tempfile::TempDir, name: &str) -> String {
    fs::read_to_string(dir.path().join(name)).expect("read")
}

#[test]
fn manang_toml_v1_loads_and_round_trips() {
    let text = fs::read_to_string(fixture("manang-toml/v1/manang.toml")).expect("fixture");
    let config = ProjectConfig::parse(&text).expect("v1 parses");
    assert_eq!(config.version, None, "v1 files carry no version key");
    assert_eq!(config.read_only_newer, None);
    insta::assert_debug_snapshot!(config);
    // Saving stamps nothing by itself and loses nothing.
    let again = ProjectConfig::parse(&config.to_toml().expect("serialise")).expect("again");
    assert_eq!(again, config);
}

#[test]
fn manang_toml_new_projects_carry_the_version() {
    let config = ProjectConfig::new(manang_project::Engine::Typst, Some("P".to_owned()));
    let text = config.to_toml().expect("serialise");
    assert!(text.starts_with("version = 1\n"), "{text}");
    assert_eq!(ProjectConfig::parse(&text).expect("parse"), config);
}

#[test]
fn manang_toml_v999_is_read_as_far_as_understood_and_never_written() {
    let text = fs::read_to_string(fixture("manang-toml/v999/manang.toml")).expect("fixture");
    let config = ProjectConfig::parse(&text).expect("a newer file still opens");
    assert_eq!(config.read_only_newer, Some(999));
    assert_eq!(config.project.name.as_deref(), Some("From the future"));
    assert_eq!(config.bibliography.paths, ["refs.bib"]);
    // Rewriting it would drop `compiler-flags`, `style` and `[collaboration]`.
    let err = config.to_toml().expect_err("read-only");
    assert!(err.to_string().contains("newer Manang"), "{err}");
}

#[test]
fn projects_toml_v1_loads_then_is_stamped_on_the_next_write_without_loss() {
    let dir = config_with("projects-toml/v1/projects.toml", REGISTRY_FILE);
    let registry = Registry::new(dir.path());
    let before = registry.all();
    assert_eq!(before.len(), 2);
    insta::assert_debug_snapshot!("projects_v1_loaded", before);
    assert!(
        registry.take_warnings().is_empty(),
        "an unversioned file is simply v1"
    );

    registry
        .update(
            "/home/ada/talks/qm",
            Patch {
                pinned: Some(true),
                ..Patch::default()
            },
        )
        .expect("update");
    let text = read(&dir, REGISTRY_FILE);
    assert!(text.starts_with("version = 1\n"), "{text}");
    insta::assert_snapshot!("projects_v1_rewritten", text);
    let after = Registry::new(dir.path()).all();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0], "the untouched entry is unchanged");
    assert!(after[1].pinned && after[1].trashed_at == before[1].trashed_at);
}

#[test]
fn projects_toml_v999_is_listed_but_never_rewritten() {
    let dir = config_with("projects-toml/v999/projects.toml", REGISTRY_FILE);
    let original = read(&dir, REGISTRY_FILE);
    let registry = Registry::new(dir.path());
    assert_eq!(registry.all().len(), 1, "what is understood is still shown");
    let warnings = registry.take_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("newer Manang")),
        "{warnings:?}"
    );
    let err = registry
        .update(
            "/home/ada/papers/thesis",
            Patch {
                pinned: Some(false),
                ..Patch::default()
            },
        )
        .expect_err("read-only");
    assert!(err.to_string().contains("newer Manang"), "{err}");
    assert!(registry.forget("/home/ada/papers/thesis").is_err());
    assert!(
        registry
            .register("/other", "Other", manang_project::Engine::Typst, 1)
            .is_err()
    );
    assert_eq!(read(&dir, REGISTRY_FILE), original, "not a byte changed");
}

#[test]
fn a_corrupt_projects_toml_is_set_aside_not_overwritten() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join(REGISTRY_FILE);
    fs::write(&file, "[[projects\npath = ").expect("write");
    let registry = Registry::new(dir.path());
    assert!(registry.all().is_empty());
    let warnings = registry.take_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("projects.toml.corrupt")),
        "{warnings:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("projects.toml.corrupt")).expect("kept"),
        "[[projects\npath = "
    );
    registry
        .register("/p", "P", manang_project::Engine::Typst, 1)
        .expect("a fresh registry works");
    assert_eq!(registry.all().len(), 1);
}

#[test]
fn recent_projects_v1_loads_and_is_stamped_on_write() {
    let dir = config_with("recent-projects-toml/v1/recent-projects.toml", RECENT_FILE);
    let store = RecentStore::new(dir.path());
    let before = store.list();
    insta::assert_debug_snapshot!("recent_v1_loaded", before);
    store
        .touch(Path::new("/home/ada/talks/qm"), "QM talk")
        .expect("touch");
    let text = read(&dir, RECENT_FILE);
    assert!(text.starts_with("version = 1\n"), "{text}");
    let after = RecentStore::new(dir.path()).list();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].path, "/home/ada/talks/qm");
    assert_eq!(after[1], before[0], "the other entry is unchanged");
}

#[test]
fn recent_projects_v999_is_never_rewritten_and_a_corrupt_one_is_set_aside() {
    let dir = config_with(
        "recent-projects-toml/v999/recent-projects.toml",
        RECENT_FILE,
    );
    let original = read(&dir, RECENT_FILE);
    let store = RecentStore::new(dir.path());
    assert_eq!(store.list().len(), 1);
    assert!(store.touch(Path::new("/new"), "New").is_err());
    assert!(store.remove(Path::new("/home/ada/papers/thesis")).is_err());
    assert_eq!(read(&dir, RECENT_FILE), original);

    let dir = tempfile::tempdir().expect("tmp");
    fs::write(dir.path().join(RECENT_FILE), "not = [toml").expect("write");
    let store = RecentStore::new(dir.path());
    assert!(store.list().is_empty());
    assert!(dir.path().join("recent-projects.toml.corrupt").exists());
    store.touch(Path::new("/b"), "b").expect("recovers");
    assert_eq!(store.list().len(), 1);
}
