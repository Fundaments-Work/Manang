//! Discovery, loading, unloading and reloading through the kernel.

use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{Kernel, KernelError, Payload};
use manang_plugins::{
    LoadError, ManagerConfig, ManagerError, PluginManager, SyncOutcome, cache_dir_in,
    plugins_dir_in,
};
use manang_wasm_host::{CacheStatus, Limits};
use serde_json::json;

mod common;
use common::{component, install, manifest_toml};

fn manager(plugins: &std::path::Path) -> (Arc<Kernel>, Arc<PluginManager>) {
    let kernel = Arc::new(Kernel::new());
    let mut config = ManagerConfig::new(plugins);
    config.limits = Limits {
        deadline: Duration::from_secs(20),
        ..Limits::default()
    };
    let manager = Arc::new(PluginManager::new(Arc::clone(&kernel), config).expect("manager"));
    (kernel, manager)
}

fn run(kernel: &Kernel, name: &str) -> Result<Payload, KernelError> {
    kernel.invoke(&format!("{name}.run"), Payload::Empty)
}

#[test]
fn default_directories_follow_the_documented_layout() {
    let config = std::path::Path::new("/home/u/.config");
    assert_eq!(
        plugins_dir_in(config),
        std::path::Path::new("/home/u/.config/manang/plugins")
    );
    assert_eq!(
        cache_dir_in(std::path::Path::new("/home/u/.cache")),
        std::path::Path::new("/home/u/.cache/manang/wasm")
    );
}

#[test]
fn scan_loads_good_plugins_and_reports_each_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    install(root, "alpha", &component('1', 1));
    install(root, "beta", &component('2', 1));
    // Not a component.
    install(root, "broken", b"definitely not wasm");
    // Directory without a manifest.
    std::fs::create_dir_all(root.join("empty")).expect("mkdir");
    // Directory name does not match the manifest.
    let mismatched = install(root, "wrong-dir", &component('3', 1));
    std::fs::write(mismatched.join("manifest.toml"), manifest_toml("other")).expect("write");
    // Bad manifest.
    let bad = install(root, "bad-manifest", &component('3', 1));
    std::fs::write(bad.join("manifest.toml"), "this is = not [valid").expect("write");
    // Entry path that tries to leave the directory.
    let escape = install(root, "escape", &component('3', 1));
    std::fs::write(
        escape.join("manifest.toml"),
        "[plugin]\nname = \"escape\"\nversion = \"0.1.0\"\nentry = \"../alpha/plugin.wasm\"\n",
    )
    .expect("write");
    // Hidden directories and loose files are ignored.
    std::fs::create_dir_all(root.join(".git")).expect("mkdir");
    std::fs::write(root.join("README.txt"), "hi").expect("write");

    let (kernel, manager) = manager(root);
    let report = manager.scan().expect("scan");

    let loaded: Vec<_> = report.loaded.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(loaded, vec!["alpha", "beta"]);
    assert_eq!(
        run(&kernel, "alpha").expect("alpha runs"),
        Payload::Json(json!(1))
    );
    assert_eq!(
        run(&kernel, "beta").expect("beta runs"),
        Payload::Json(json!(2))
    );

    let mut failed: Vec<_> = report
        .failed
        .iter()
        .map(|f| {
            (
                f.dir
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
                &f.error,
            )
        })
        .collect();
    failed.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<_> = failed.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["bad-manifest", "broken", "empty", "escape", "wrong-dir"]
    );
    assert!(matches!(failed[0].1, LoadError::Manifest(_)));
    assert!(matches!(failed[1].1, LoadError::Wasm(_)));
    assert!(matches!(failed[2].1, LoadError::NoManifest(_)));
    assert!(
        matches!(failed[3].1, LoadError::Manifest(_)),
        "{:?}",
        failed[3].1
    );
    assert!(matches!(failed[4].1, LoadError::NameMismatch { .. }));
    // Failure messages are human readable.
    assert!(failed.iter().all(|(_, e)| !e.to_string().is_empty()));
}

#[cfg(unix)]
#[test]
fn an_entry_symlinked_out_of_the_plugin_directory_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("plugins");
    let outside = tmp.path().join("outside.wasm");
    std::fs::write(&outside, component('9', 1)).expect("write");
    let dir = install(&root, "sneaky", b"placeholder");
    std::fs::remove_file(dir.join("plugin.wasm")).expect("rm");
    std::os::unix::fs::symlink(&outside, dir.join("plugin.wasm")).expect("symlink");

    let (_kernel, manager) = manager(&root);
    let report = manager.scan().expect("scan");
    assert!(report.loaded.is_empty());
    assert!(
        matches!(report.failed[0].error, LoadError::EntryEscapes(_)),
        "{:?}",
        report.failed[0].error
    );
}

#[test]
fn scan_twice_is_idempotent_and_picks_up_new_plugins() {
    let tmp = tempfile::tempdir().expect("tempdir");
    install(tmp.path(), "alpha", &component('1', 1));
    let (_kernel, manager) = manager(tmp.path());
    assert_eq!(manager.scan().expect("scan").loaded.len(), 1);
    let again = manager.scan().expect("scan");
    assert!(again.loaded.is_empty() && again.failed.is_empty());
    install(tmp.path(), "beta", &component('2', 1));
    assert_eq!(manager.scan().expect("scan").loaded.len(), 1);
    assert_eq!(manager.plugins().len(), 2);
}

#[test]
fn unload_removes_commands_but_keeps_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = install(tmp.path(), "alpha", &component('1', 1));
    let (kernel, manager) = manager(tmp.path());
    manager.scan().expect("scan");
    assert!(run(&kernel, "alpha").is_ok());

    manager.unload("alpha").expect("unloads");
    assert!(matches!(
        run(&kernel, "alpha"),
        Err(KernelError::UnknownCommand(_))
    ));
    assert!(dir.join("plugin.wasm").exists());
    assert!(matches!(
        manager.unload("alpha"),
        Err(ManagerError::NotLoaded(_))
    ));
    // Loading it again works.
    assert!(matches!(manager.sync("alpha"), SyncOutcome::Loaded(_)));
    assert!(run(&kernel, "alpha").is_ok());
}

#[test]
fn reload_swaps_versions_and_detects_no_change() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = install(tmp.path(), "alpha", &component('1', 1));
    let (kernel, manager) = manager(tmp.path());
    manager.scan().expect("scan");
    assert_eq!(run(&kernel, "alpha").expect("v1"), Payload::Json(json!(1)));

    assert!(matches!(
        manager.reload("alpha").expect("loaded"),
        SyncOutcome::Unchanged(_)
    ));
    std::fs::write(dir.join("plugin.wasm"), component('2', 1)).expect("write v2");
    assert!(matches!(
        manager.reload("alpha").expect("loaded"),
        SyncOutcome::Reloaded(_)
    ));
    assert_eq!(run(&kernel, "alpha").expect("v2"), Payload::Json(json!(2)));

    // A broken update is reported and the working version stays.
    std::fs::write(dir.join("plugin.wasm"), b"truncated").expect("write junk");
    match manager.reload("alpha").expect("loaded") {
        SyncOutcome::Failed(f) => assert!(matches!(f.error, LoadError::Wasm(_))),
        other => panic!("expected failure, got {other:?}"),
    }
    assert_eq!(
        run(&kernel, "alpha").expect("still v2"),
        Payload::Json(json!(2))
    );
    assert!(matches!(
        manager.reload("nope"),
        Err(ManagerError::NotLoaded(_))
    ));
}

#[test]
fn an_in_flight_call_finishes_on_the_old_instance_during_reload() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // v1 spins for a while (billions of iterations) before answering.
    let dir = install(tmp.path(), "slow", &component('1', 2_000_000_000));
    let (kernel, manager) = manager(tmp.path());
    manager.scan().expect("scan");

    let worker = {
        let kernel = Arc::clone(&kernel);
        std::thread::spawn(move || run(&kernel, "slow"))
    };
    std::thread::sleep(Duration::from_millis(100));
    std::fs::write(dir.join("plugin.wasm"), component('2', 1)).expect("write v2");
    assert!(matches!(
        manager.reload("slow").expect("loaded"),
        SyncOutcome::Reloaded(_)
    ));
    assert!(
        !worker.is_finished(),
        "the reload must happen while the old call is still running"
    );
    // New calls already see v2 ...
    assert_eq!(run(&kernel, "slow").expect("v2"), Payload::Json(json!(2)));
    // ... and the old call completes on v1.
    assert_eq!(
        worker.join().expect("joined").expect("old call finishes"),
        Payload::Json(json!(1))
    );
}

#[test]
fn the_precompiled_cache_serves_the_second_start() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plugins = tmp.path().join("plugins");
    install(&plugins, "alpha", &component('1', 1));
    let config = || {
        let mut c = ManagerConfig::new(&plugins);
        c.cache_dir = Some(tmp.path().join("cache"));
        c
    };
    let first = PluginManager::new(Arc::new(Kernel::new()), config()).expect("manager");
    let report = first.scan().expect("scan");
    assert_eq!(report.loaded[0].cache, CacheStatus::Miss);

    let second = PluginManager::new(Arc::new(Kernel::new()), config()).expect("manager");
    let report = second.scan().expect("scan");
    assert_eq!(report.loaded[0].cache, CacheStatus::Hit);
}

#[test]
fn plugins_cannot_steal_each_others_commands() {
    let tmp = tempfile::tempdir().expect("tempdir");
    install(tmp.path(), "alpha", &component('1', 1));
    let thief = install(tmp.path(), "bravo", &component('2', 1));
    // `bravo` declares `alpha.run`, which fails manifest namespacing.
    std::fs::write(
        thief.join("manifest.toml"),
        "[plugin]\nname = \"bravo\"\nversion = \"0.1.0\"\n[[contributes.commands]]\nid = \"alpha.run\"\ntitle = \"x\"\n",
    )
    .expect("write");
    let (kernel, manager) = manager(tmp.path());
    let report = manager.scan().expect("scan");
    assert_eq!(report.loaded.len(), 1);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(
        run(&kernel, "alpha").expect("alpha"),
        Payload::Json(json!(1))
    );
}
