//! Golden-fixture migration tests for figure scenes (`*.figure.json`,
//! `docs/formats.md`).
//!
//! `testdata/formats/figure/v1` and `v2` are scenes as earlier releases wrote
//! them; `v999` is a made-up future version. Opening an old scene migrates it in
//! memory; saving keeps the old file once as `<name>.figure.json.bak-v<N>`; a
//! newer scene is refused (so it can never be overwritten) and left untouched.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use manang_figure::{FigureStore, SCENE_VERSION};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/formats/figure")
        .join(rel)
}

fn copy_in(rel: &str, dir: &Path) -> PathBuf {
    let target = dir.join("scene.figure.json");
    fs::copy(fixture(rel), &target).expect("copy fixture");
    target
}

#[test]
fn a_v1_scene_migrates_and_the_old_file_is_kept_once_on_save() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = copy_in("v1/scene.figure.json", dir.path());
    let original = fs::read_to_string(&path).expect("read");

    let store = FigureStore::new();
    let id = store.open(&path).expect("v1 opens");
    let scene = store.get(Some(id)).expect("scene");
    assert_eq!(scene.version, SCENE_VERSION);
    insta::assert_json_snapshot!("figure_v1_migrated", scene);
    // Opening changes nothing on disk.
    assert_eq!(fs::read_to_string(&path).expect("read"), original);

    store.save(Some(id), None, &BTreeMap::new()).expect("save");
    let backup = dir.path().join("scene.figure.json.bak-v1");
    assert_eq!(
        fs::read_to_string(&backup).expect("backup"),
        original,
        "the old file is kept byte for byte"
    );
    let saved = fs::read_to_string(&path).expect("read");
    assert!(saved.starts_with("{\n  \"version\": 2,"), "{saved}");

    // A second save does not replace the backup with the migrated text.
    store
        .save(Some(id), None, &BTreeMap::new())
        .expect("save again");
    assert_eq!(fs::read_to_string(&backup).expect("backup"), original);

    // The saved file reads back to the same scene.
    let again = FigureStore::new();
    let id2 = again.open(&path).expect("reopen");
    assert_eq!(again.get(Some(id2)).expect("scene"), scene);
    assert!(
        !dir.path().join("scene.figure.json.bak-v2").exists(),
        "a current file needs no backup"
    );
}

#[test]
fn a_v2_scene_with_old_formulas_is_read_and_saved_as_a_current_one() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = copy_in("v2/scene.figure.json", dir.path());
    let store = FigureStore::new();
    let id = store.open(&path).expect("v2 opens");
    insta::assert_json_snapshot!("figure_v2_read", store.get(Some(id)).expect("scene"));
    store.save(Some(id), None, &BTreeMap::new()).expect("save");
    let saved = fs::read_to_string(&path).expect("read");
    assert!(
        saved.contains("\"source\": \"E = mc^2\"") && !saved.contains("\"tex\""),
        "{saved}"
    );
    assert!(
        !dir.path().join("scene.figure.json.bak-v2").exists(),
        "the current version is not backed up"
    );
}

#[test]
fn a_v999_scene_is_refused_and_not_one_byte_changes() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = copy_in("v999/scene.figure.json", dir.path());
    let original = fs::read(&path).expect("read");
    let store = FigureStore::new();
    let err = store
        .open(&path)
        .expect_err("a newer scene cannot be opened");
    assert!(err.to_string().contains("version 999"), "{err}");
    assert!(err.to_string().contains("Update Manang"), "{err}");
    assert_eq!(fs::read(&path).expect("read"), original);
    assert!(!dir.path().join("scene.svg").exists());
}

#[test]
fn saving_is_atomic_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("fresh.figure.json");
    let store = FigureStore::new();
    let (id, _) = store.create_at(&path, None, None).expect("create");
    store.save(Some(id), None, &BTreeMap::new()).expect("save");
    let names: Vec<String> = fs::read_dir(dir.path())
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !n.starts_with(".manang-tmp-")),
        "{names:?}"
    );
}
