//! The `plugins.*` commands, driven through the kernel like the shell does.

use std::sync::Arc;

use manang_kernel::{Kernel, KernelError, Payload, PluginError};
use manang_plugins::{ManagerConfig, PluginManager};
use serde_json::{Value, json};

mod common;
use common::{archive, component, folder, manifest_with};

struct Rig {
    tmp: tempfile::TempDir,
    kernel: Arc<Kernel>,
    // Held so the weakly-referencing plugin keeps working.
    _manager: Arc<PluginManager>,
}

fn rig() -> Rig {
    let tmp = tempfile::tempdir().expect("tempdir");
    let kernel = Arc::new(Kernel::new());
    let mut config = ManagerConfig::new(tmp.path().join("plugins"));
    config.registry_path = Some(tmp.path().join("config/plugins.toml"));
    config.trash_dir = Some(tmp.path().join("trash"));
    let manager = Arc::new(PluginManager::new(Arc::clone(&kernel), config).expect("manager"));
    kernel
        .register(
            manang_plugins::manifest().expect("manifest"),
            Arc::new(manang_plugins::plugin(&manager)),
        )
        .expect("register plugins plugin");
    Rig {
        tmp,
        kernel,
        _manager: manager,
    }
}

fn call(rig: &Rig, command: &str, args: Value) -> Result<Value, String> {
    match rig.kernel.invoke(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(other) => panic!("expected JSON, got {other:?}"),
        Err(KernelError::Plugin(PluginError::Failed(m))) => Err(m),
        Err(other) => Err(other.to_string()),
    }
}

#[test]
fn the_manifest_declares_every_command() {
    let manifest = manang_plugins::manifest().expect("valid manifest");
    let ids: Vec<&str> = manifest
        .contributes
        .commands
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    for expected in [
        "plugins.list",
        "plugins.inspect",
        "plugins.install",
        "plugins.approve",
        "plugins.enable",
        "plugins.disable",
        "plugins.uninstall",
        "plugins.reload",
        "plugins.permissions",
        "plugins.use-project",
    ] {
        assert!(ids.contains(&expected), "missing {expected}");
    }
}

#[test]
fn the_whole_lifecycle_through_commands() {
    let rig = rig();
    let src = folder(
        &rig.tmp.path().join("src"),
        "alpha",
        &manifest_with("alpha", "0.1.0", &["example.org"]),
        &component('1', 1),
    );
    let path = src.to_string_lossy().into_owned();

    let inspected = call(&rig, "plugins.inspect", json!({ "path": path })).expect("inspect");
    assert_eq!(inspected["name"], "alpha");
    assert_eq!(inspected["permissions"]["network"], json!(["example.org"]));

    let installed = call(&rig, "plugins.install", json!({ "path": path })).expect("install");
    assert_eq!(installed["status"], "needs_approval");

    let approved = call(
        &rig,
        "plugins.approve",
        json!({ "name": "alpha", "permissions": { "network": ["example.org"] } }),
    )
    .expect("approve");
    assert_eq!(approved["status"], "loaded");
    assert_eq!(
        rig.kernel
            .invoke("alpha.run", Payload::Empty)
            .expect("runs"),
        Payload::Json(json!(1))
    );

    let list = call(&rig, "plugins.list", json!({})).expect("list");
    let names: Vec<&str> = list["plugins"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|p| p["name"].as_str())
        .collect();
    assert_eq!(names, ["alpha", "plugins"], "built-ins are listed too");
    let own = &list["plugins"][1];
    assert_eq!(own["tier"], "native");
    assert_eq!(own["core"], true);

    let permissions =
        call(&rig, "plugins.permissions", json!({ "name": "alpha" })).expect("permissions");
    assert_eq!(permissions["granted"]["network"], json!(["example.org"]));

    let off = call(&rig, "plugins.disable", json!({ "name": "alpha" })).expect("disable");
    assert_eq!(off["status"], "disabled");
    let on = call(&rig, "plugins.enable", json!({ "name": "alpha" })).expect("enable");
    assert_eq!(on["status"], "loaded");
    let again = call(&rig, "plugins.reload", json!({ "name": "alpha" })).expect("reload");
    assert_eq!(again["status"], "loaded");

    let gone = call(&rig, "plugins.uninstall", json!({ "name": "alpha" })).expect("uninstall");
    assert!(gone["trashed_to"].is_string());
    assert!(rig.kernel.invoke("alpha.run", Payload::Empty).is_err());

    let project = rig.tmp.path().join("project");
    std::fs::create_dir_all(&project).expect("mkdir");
    let used = call(
        &rig,
        "plugins.use-project",
        json!({ "path": project.to_string_lossy() }),
    )
    .expect("use-project");
    assert_eq!(used["changed"], json!([]));
    call(&rig, "plugins.use-project", json!({ "path": null })).expect("clear");
}

#[test]
fn errors_are_coded_and_arguments_are_strict() {
    let rig = rig();
    let err = call(&rig, "plugins.enable", json!({ "name": "../x" })).expect_err("bad name");
    assert!(err.starts_with("[invalid_name]"), "{err}");
    let err = call(&rig, "plugins.disable", json!({ "name": "plugins" })).expect_err("core");
    assert!(err.starts_with("[core_plugin]"), "{err}");
    let err = call(&rig, "plugins.uninstall", json!({ "name": "plugins" })).expect_err("core");
    assert!(err.starts_with("[core_plugin]"), "{err}");
    let err = call(&rig, "plugins.enable", json!({ "name": "nope" })).expect_err("missing");
    assert!(err.starts_with("[not_installed]"), "{err}");
    let err = call(&rig, "plugins.enable", json!({ "name": "a", "extra": 1 })).expect_err("extra");
    assert!(err.starts_with("[bad_arguments]"), "{err}");
    let err = call(&rig, "plugins.enable", json!({})).expect_err("missing field");
    assert!(err.starts_with("[bad_arguments]"), "{err}");
    let err = call(&rig, "plugins.list", json!({ "x": 1 })).expect_err("list takes none");
    assert!(err.starts_with("[bad_arguments]"), "{err}");
    let err = call(
        &rig,
        "plugins.install",
        json!({ "path": "/no/such/plugin" }),
    )
    .expect_err("path");
    assert!(err.starts_with("[invalid_path]"), "{err}");

    let bad = archive(
        &rig.tmp.path().join("evil.manangplugin"),
        &[("manifest.toml", b"x"), ("../up.txt", b"x")],
    );
    let err = call(
        &rig,
        "plugins.install",
        json!({ "path": bad.to_string_lossy() }),
    )
    .expect_err("traversal");
    assert!(err.starts_with("[archive]"), "{err}");
}
