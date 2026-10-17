//! The precompiled-component cache: hits, and refusal of anything tampered.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use manang_kernel::{Kernel, Payload};
use manang_wasm_host::{CacheStatus, Limits, RuntimeOptions, WasmRuntime};

mod common;
use common::{echo_guest, io_guest, manifest};

fn runtime(dir: &Path, pooling: bool) -> WasmRuntime {
    WasmRuntime::with_options(RuntimeOptions {
        limits: Limits::default(),
        pooling,
        pool_slots: 4,
        cache_dir: Some(dir.to_path_buf()),
    })
    .expect("engine")
}

fn echo_manifest() -> manang_kernel::Manifest {
    manifest(
        "[plugin]\nname = \"echo\"\nversion = \"0.1.0\"\n\
         [permissions]\nbuffer = [\"read:active\"]\n\
         [[contributes.commands]]\nid = \"echo.say\"\ntitle = \"say\"\n",
    )
}

/// Load the echo guest through `rt`, check it works, return the cache status.
fn load_echo(rt: &WasmRuntime) -> CacheStatus {
    let m = echo_manifest();
    let plugin = rt.load(&m, echo_guest(), None).expect("loads");
    let status = plugin.cache_status().clone();
    let k = Kernel::new();
    k.register(m, Arc::new(plugin)).expect("registers");
    assert_eq!(
        k.invoke("echo.say", Payload::Empty).expect("works"),
        Payload::Empty
    );
    status
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .expect("read cache dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cwasm"))
        .collect();
    out.sort();
    out
}

fn rejected(status: &CacheStatus) -> bool {
    matches!(status, CacheStatus::Rejected(_))
}

#[test]
fn second_start_is_served_from_the_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    echo_guest(); // build the guest outside the timed region
    let t = Instant::now();
    assert_eq!(load_echo(&runtime(dir.path(), false)), CacheStatus::Miss);
    let cold = t.elapsed();
    assert_eq!(entries(dir.path()).len(), 1);

    // A brand new engine, as after an app restart.
    let t = Instant::now();
    assert_eq!(load_echo(&runtime(dir.path(), false)), CacheStatus::Hit);
    let warm = t.elapsed();
    eprintln!("cold load (compile): {cold:?}, warm load (deserialise): {warm:?}");
    assert!(warm < cold, "cache must be faster: {warm:?} vs {cold:?}");
}

#[test]
fn corrupted_entries_are_rejected_and_replaced() {
    let dir = tempfile::tempdir().expect("tempdir");
    load_echo(&runtime(dir.path(), false));
    let entry = entries(dir.path()).remove(0);
    let original = std::fs::read(&entry).expect("read");

    // Flip one byte in the middle of the machine code, then in the MAC, then in
    // the header, then truncate.
    for position in [original.len() / 2, 12, 2] {
        let mut bytes = original.clone();
        bytes[position] ^= 0x01;
        std::fs::write(&entry, &bytes).expect("write");
        assert!(
            rejected(&load_echo(&runtime(dir.path(), false))),
            "flip at {position}"
        );
        // The bad entry was replaced by a good one.
        assert_eq!(load_echo(&runtime(dir.path(), false)), CacheStatus::Hit);
    }
    std::fs::write(&entry, &original[..original.len() / 2]).expect("write");
    assert!(rejected(&load_echo(&runtime(dir.path(), false))));
    std::fs::write(&entry, b"").expect("write");
    assert!(rejected(&load_echo(&runtime(dir.path(), false))));
    assert_eq!(load_echo(&runtime(dir.path(), false)), CacheStatus::Hit);
}

#[test]
fn an_entry_copied_from_another_plugin_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rt = runtime(dir.path(), false);
    load_echo(&rt);
    let echo_entry = entries(dir.path()).remove(0);
    // Populate the entry for a different component, then overwrite the echo
    // entry with it. Both carry valid MACs, but for different content hashes.
    let m = manifest(
        "[plugin]\nname = \"io\"\nversion = \"0.1.0\"\n\
         [permissions]\nfilesystem = [\"read:a\"]\nnetwork = [\"a.org\"]\n",
    );
    rt.load(&m, io_guest(), None).expect("loads io guest");
    let io_entry = entries(dir.path())
        .into_iter()
        .find(|p| *p != echo_entry)
        .expect("second entry");
    std::fs::copy(&io_entry, &echo_entry).expect("swap");
    assert!(rejected(&load_echo(&runtime(dir.path(), false))));
}

#[test]
fn artefacts_forged_without_the_key_are_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    load_echo(&runtime(dir.path(), false));
    let entry = entries(dir.path()).remove(0);

    // Replace the whole entry with a validly shaped file whose MAC is made up.
    let mut forged = b"MNGCWASM".to_vec();
    forged.extend_from_slice(&[0xAB; 32]);
    forged.extend_from_slice(b"\x7fELF not really machine code");
    std::fs::write(&entry, &forged).expect("write");
    assert!(rejected(&load_echo(&runtime(dir.path(), false))));

    // Rotating the key invalidates everything produced under the old one.
    load_echo(&runtime(dir.path(), false));
    std::fs::remove_file(dir.path().join("cache.key")).expect("remove key");
    assert!(rejected(&load_echo(&runtime(dir.path(), false))));
}

#[cfg(unix)]
#[test]
fn the_key_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    load_echo(&runtime(dir.path(), false));
    let mode = std::fs::metadata(dir.path().join("cache.key"))
        .expect("key")
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "mode {mode:o}");
}

#[test]
fn a_wrong_length_key_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("cache.key"), b"short").expect("write");
    assert!(
        WasmRuntime::with_options(RuntimeOptions {
            cache_dir: Some(dir.path().to_path_buf()),
            ..RuntimeOptions::default()
        })
        .is_err()
    );
}

#[test]
fn pooling_and_on_demand_engines_share_a_cache_safely() {
    let dir = tempfile::tempdir().expect("tempdir");
    load_echo(&runtime(dir.path(), false));
    // Whatever the fingerprint decides, both configurations must work.
    let status = load_echo(&runtime(dir.path(), true));
    assert!(!rejected(&status), "{status:?}");
}
