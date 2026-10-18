//! Golden-fixture tests for recovery snapshots (`docs/formats.md`): a v1
//! snapshot as the first release wrote it is read back, and a snapshot from a
//! made-up v999 is left alone and never offered.

use std::fs;
use std::path::{Path, PathBuf};

use manang_recovery::{RecoveryStore, WriteRequest};

const PROJECT: &str = "/home/ada/papers/thesis";

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/formats/recovery")
        .join(rel)
}

/// A store with one snapshot written through the API (which creates the
/// project's folder), returning that folder so fixtures can be dropped in.
fn store_with_folder() -> (tempfile::TempDir, RecoveryStore, PathBuf) {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("recovery");
    let store = RecoveryStore::new(&root);
    store
        .write(
            serde_json::from_value::<WriteRequest>(serde_json::json!({
                "project": PROJECT, "path": "other.typ", "kind": "text", "content": "x"
            }))
            .expect("request"),
        )
        .expect("write");
    let folder = fs::read_dir(&root)
        .expect("root")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("project folder");
    (tmp, store, folder)
}

#[test]
fn a_v1_snapshot_is_read_back_whole() {
    let (_tmp, store, folder) = store_with_folder();
    fs::copy(fixture("v1/snapshot.json"), folder.join("fixture-v1.json")).expect("copy");
    let listing = store.list(PROJECT);
    assert_eq!(listing.set_aside, 0);
    let paths: Vec<_> = listing.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"chapters/intro.typ"), "{paths:?}");
    let direct: manang_recovery::Snapshot =
        serde_json::from_str(&fs::read_to_string(fixture("v1/snapshot.json")).expect("read"))
            .expect("fixture parses");
    assert_eq!(direct.version, 1);
    insta::assert_json_snapshot!("recovery_v1", direct);
}

#[test]
fn a_v999_snapshot_is_never_offered_and_not_one_byte_changes() {
    let (_tmp, store, folder) = store_with_folder();
    let target = folder.join("fixture-v999.json");
    fs::copy(fixture("v999/snapshot.json"), &target).expect("copy");
    let before = fs::read(&target).expect("read");
    let listing = store.list(PROJECT);
    assert!(
        listing
            .entries
            .iter()
            .all(|e| e.path != "chapters/future.typ"),
        "a newer snapshot is not offered"
    );
    assert_eq!(listing.set_aside, 0, "and it is not set aside either");
    assert_eq!(fs::read(&target).expect("read"), before);
    // Discarding the project's snapshots removes only what this build wrote.
    store.discard(PROJECT, None).expect("discard");
    assert!(
        target.exists(),
        "a newer Manang's snapshot survives a discard"
    );
}
