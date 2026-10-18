//! The project registry commands, driven through the real kernel on temp dirs.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{EventBus, Kernel, Payload};
use serde_json::{Value, json};

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    work: PathBuf,
    config: PathBuf,
}

fn kernel_on(config: &Path) -> Kernel {
    let kernel = Kernel::new();
    let plugin = manang_project::ProjectPlugin::new(config, EventBus::new())
        .with_debounce(Duration::from_millis(50));
    kernel
        .register(
            manang_project::manifest().expect("manifest"),
            Arc::new(plugin),
        )
        .expect("register");
    kernel
}

fn harness() -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let work = tmp.path().join("work");
    let config = tmp.path().join("config");
    fs::create_dir_all(&work).expect("work");
    let kernel = kernel_on(&config);
    Harness {
        _tmp: tmp,
        kernel,
        work,
        config,
    }
}

impl Harness {
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(Payload::Empty) => Ok(Value::Null),
            Ok(Payload::Binary(_)) => Err("binary".to_owned()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        self.call(command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    fn create(&self, name: &str, engine: &str) -> String {
        let dir = self.work.join(name).to_string_lossy().into_owned();
        self.ok(
            "project.create",
            json!({ "path": dir, "name": name, "engine": engine }),
        );
        dir
    }
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .expect("array")
        .iter()
        .filter_map(|e| e["name"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn open_and_create_register_projects_with_metadata() {
    let h = harness();
    h.create("One", "latex");
    let list = h.ok("project.list", json!({}));
    assert_eq!(names(&list), ["One"]);
    assert_eq!(list[0]["engine"], "latex");
    assert_eq!(list[0]["exists"], true);
    assert_eq!(list[0]["pinned"], false);
    assert_eq!(list[0]["has_thumbnail"], false);
    let text = fs::read_to_string(h.config.join("projects.toml")).expect("toml");
    assert!(text.contains("[[projects]]") && text.contains("engine = \"latex\""));
}

#[test]
fn register_update_archive_sort_and_forget() {
    let h = harness();
    let _ = h.create("Bravo", "typst");
    let alpha = h.create("alpha", "typst");
    let _ = h.create("Charlie", "typst");
    h.ok("project.close", json!({}));
    let bravo = h.work.join("Bravo").to_string_lossy().into_owned();

    let by_name = h.ok("project.list", json!({ "sort": "name" }));
    assert_eq!(names(&by_name), ["alpha", "Bravo", "Charlie"]);

    let updated = h.ok(
        "project.update",
        json!({ "path": alpha, "name": "Alpha paper", "pinned": true, "tags": ["phd", "draft"] }),
    );
    assert_eq!(updated["name"], "Alpha paper");
    assert_eq!(updated["tags"], json!(["phd", "draft"]));
    h.ok("project.update", json!({ "path": bravo, "archived": true }));

    let visible = h.ok("project.list", json!({ "sort": "name" }));
    assert_eq!(names(&visible), ["Alpha paper", "Charlie"]);
    let all = h.ok(
        "project.list",
        json!({ "include_archived": true, "sort": "name" }),
    );
    assert_eq!(names(&all), ["Alpha paper", "Bravo", "Charlie"]);
    let found = h.ok("project.list", json!({ "query": "DRAFT" }));
    assert_eq!(names(&found), ["Alpha paper"]);

    // A rename survives reopening the project.
    h.ok("project.open", json!({ "path": alpha }));
    assert_eq!(
        names(&h.ok("project.list", json!({ "sort": "name" }))),
        ["Alpha paper", "Charlie"]
    );

    // Bad metadata is rejected and nothing changes.
    let err = h
        .call("project.update", json!({ "path": alpha, "name": "  " }))
        .expect_err("empty name");
    assert!(err.contains("name cannot be empty"), "{err}");
    let err = h
        .call("project.update", json!({ "path": "/nope", "pinned": true }))
        .expect_err("unknown");
    assert!(err.contains("does not exist"), "{err}");

    // Forgetting only edits the list; the folder stays.
    assert_eq!(
        h.ok("project.forget", json!({ "path": bravo })),
        json!({ "removed": true })
    );
    assert!(Path::new(&bravo).join("manang.toml").is_file());
    assert_eq!(
        h.ok("project.forget", json!({ "path": bravo })),
        json!({ "removed": false })
    );
    // Registering it again brings it back.
    let back = h.ok("project.register", json!({ "path": bravo }));
    assert_eq!(back["name"], "Bravo");
    assert_eq!(back["archived"], false);
    let err = h
        .call("project.register", json!({ "path": h.work }))
        .expect_err("not a project");
    assert!(err.contains("not a Manang project"), "{err}");
}

#[test]
fn missing_folders_are_reported_not_removed() {
    let h = harness();
    let dir = h.create("Gone", "markdown");
    h.ok("project.close", json!({}));
    fs::remove_dir_all(&dir).expect("rm");
    let list = h.ok("project.list", json!({}));
    assert_eq!(names(&list), ["Gone"]);
    assert_eq!(list[0]["exists"], false);
}

#[test]
fn thumbnails_round_trip_as_binary_and_are_size_capped() {
    let h = harness();
    let dir = h.create("Paper", "typst");
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"/>";
    assert_eq!(
        h.ok("project.thumbnail-get", json!({ "path": dir })),
        Value::Null
    );
    h.ok("project.thumbnail-set", json!({ "path": dir, "svg": svg }));
    match h.kernel.invoke(
        "project.thumbnail-get",
        Payload::Json(json!({ "path": dir })),
    ) {
        Ok(Payload::Binary(b)) => assert_eq!(&*b, svg.as_bytes()),
        other => panic!("expected bytes, got {other:?}"),
    }
    assert_eq!(h.ok("project.list", json!({}))[0]["has_thumbnail"], true);

    let huge = format!("<svg>{}</svg>", "x".repeat(2 * 1024 * 1024));
    let err = h
        .call("project.thumbnail-set", json!({ "path": dir, "svg": huge }))
        .expect_err("too big");
    assert!(err.contains("byte limit"), "{err}");
    let err = h
        .call(
            "project.thumbnail-set",
            json!({ "path": "/not/registered", "svg": svg }),
        )
        .expect_err("unregistered");
    assert!(err.contains("does not exist"), "{err}");

    h.ok("project.forget", json!({ "path": dir }));
    assert_eq!(
        h.ok("project.thumbnail-get", json!({ "path": dir })),
        Value::Null
    );
}

#[test]
fn recent_entries_migrate_into_a_fresh_registry() {
    let tmp = tempfile::tempdir().expect("tmp");
    let config = tmp.path().join("config");
    let proj = tmp.path().join("old");
    fs::create_dir_all(&config).expect("cfg");
    fs::create_dir_all(&proj).expect("proj");
    fs::write(
        proj.join("manang.toml"),
        "[project]\nengine = \"markdown\"\n",
    )
    .expect("toml");
    fs::write(
        config.join("recent-projects.toml"),
        format!(
            "[[projects]]\npath = {:?}\nname = \"Old\"\nopened_at = 7\n\n[[projects]]\npath = \"/gone\"\nname = \"Gone\"\nopened_at = 5\n",
            proj.to_string_lossy()
        ),
    )
    .expect("recent");
    let kernel = kernel_on(&config);
    let Ok(Payload::Json(list)) = kernel.invoke("project.list", Payload::Json(json!({}))) else {
        panic!("list failed");
    };
    assert_eq!(names(&list), ["Old"]);
    assert_eq!(list[0]["engine"], "markdown");
    assert_eq!(list[0]["last_opened"], 7);
}

#[test]
fn trash_and_restore_via_kernel() {
    let h = harness();
    let alpha = h.create("Alpha", "typst");
    let normal_list = h.ok("project.list", json!({}));
    assert_eq!(names(&normal_list), ["Alpha"]);

    let trashed = h.ok("project.trash", json!({ "path": alpha }));
    assert!(trashed["trashed_at"].is_string());

    // Filtered out from default list
    let list_after = h.ok("project.list", json!({}));
    assert_eq!(names(&list_after).len(), 0);

    // Visible when include_trashed: true
    let list_trashed = h.ok("project.list", json!({ "include_trashed": true }));
    assert_eq!(names(&list_trashed), ["Alpha"]);
    assert_eq!(list_trashed[0]["trashed_at"], trashed["trashed_at"]);

    // Restore brings it back
    let restored = h.ok("project.restore", json!({ "path": alpha }));
    assert_eq!(restored["trashed_at"], Value::Null);

    let list_restored = h.ok("project.list", json!({}));
    assert_eq!(names(&list_restored), ["Alpha"]);

    // Trashing/restoring unknown project errors with not found
    let err = h
        .call("project.trash", json!({ "path": "/unknown" }))
        .expect_err("unknown");
    assert!(err.contains("does not exist"), "{err}");
    let err = h
        .call("project.restore", json!({ "path": "/unknown" }))
        .expect_err("unknown");
    assert!(err.contains("does not exist"), "{err}");
}

#[test]
fn purge_with_and_without_delete_files() {
    let h = harness();
    let beta = h.create("Beta", "typst");
    let gamma = h.create("Gamma", "latex");

    // Purge Beta with delete_files: false
    let res = h.ok(
        "project.purge",
        json!({ "path": beta, "delete_files": false }),
    );
    assert_eq!(res, json!({ "purged": true, "deleted_files": false }));
    // Removed from registry
    let list = h.ok("project.list", json!({ "include_trashed": true }));
    assert_eq!(names(&list), ["Gamma"]);
    // Folder STILL exists on disk
    assert!(Path::new(&beta).join("manang.toml").is_file());

    // Purge Gamma with delete_files: true
    let res = h.ok(
        "project.purge",
        json!({ "path": gamma, "delete_files": true }),
    );
    assert_eq!(res, json!({ "purged": true, "deleted_files": true }));
    // Removed from registry
    let list = h.ok("project.list", json!({ "include_trashed": true }));
    assert_eq!(names(&list).len(), 0);
    // Folder in original location is GONE (moved to system trash)
    assert!(!Path::new(&gamma).exists());

    // Purging non-existent project errors
    let err = h
        .call("project.purge", json!({ "path": "/unknown" }))
        .expect_err("unknown");
    assert!(err.contains("does not exist"), "{err}");
}

#[test]
fn empty_trash_via_kernel() {
    let h = harness();
    let one = h.create("One", "typst");
    let two = h.create("Two", "typst");
    let _three = h.create("Three", "typst");

    h.ok("project.trash", json!({ "path": one }));
    h.ok("project.trash", json!({ "path": two }));

    // Empty trash removes all trashed projects
    let res = h.ok("project.empty-trash", json!({}));
    assert_eq!(res["count"], 2);

    let list = h.ok("project.list", json!({ "include_trashed": true }));
    assert_eq!(names(&list), ["Three"]);
}
