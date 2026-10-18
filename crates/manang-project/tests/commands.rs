//! Every `project.*` command, driven through the real kernel on temp dirs.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{EventBus, Kernel, KernelError, Payload};
use serde_json::{Value, json};

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    bus: EventBus,
    work: std::path::PathBuf,
    config: std::path::PathBuf,
}

fn harness() -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let work = tmp.path().join("work");
    let config = tmp.path().join("config");
    fs::create_dir_all(&work).expect("work");
    let bus = EventBus::new();
    let kernel = Kernel::new();
    let plugin = manang_project::ProjectPlugin::new(&config, bus.clone())
        .with_debounce(Duration::from_millis(50));
    kernel
        .register(
            manang_project::manifest().expect("manifest"),
            Arc::new(plugin),
        )
        .expect("register");
    Harness {
        _tmp: tmp,
        kernel,
        bus,
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

    fn create(&self, engine: &str) -> String {
        let dir = self.work.join("paper");
        let dir = dir.to_string_lossy().into_owned();
        self.ok(
            "project.create",
            json!({ "path": dir, "name": "Paper", "engine": engine }),
        );
        dir
    }
}

#[test]
fn manifest_matches_the_commands_the_plugin_handles() {
    let h = harness();
    let ids: Vec<String> = h.kernel.commands().into_iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), 22);
    // Every declared command is routed (an unknown one would error with UnknownCommand).
    for id in ids {
        if let Err(err) = h.call(&id, json!({ "bogus": 1 })) {
            assert!(!err.contains("does not handle"), "{id} is unrouted: {err}");
        }
    }
}

#[test]
fn commands_other_than_open_need_an_open_project() {
    let h = harness();
    for command in [
        "project.tree",
        "project.read-file",
        "project.write-file",
        "project.rename",
        "project.delete",
        "project.git-status",
        "project.config",
    ] {
        let args = json!({ "path": "a", "content": "", "from": "a", "to": "b" });
        let args = match command {
            "project.tree" | "project.git-status" | "project.config" => json!({}),
            "project.rename" => json!({ "from": "a", "to": "b" }),
            "project.write-file" => json!({ "path": "a", "content": "" }),
            _ => json!({ "path": args["path"] }),
        };
        let err = h.call(command, args).expect_err("no project");
        assert!(err.contains("no project is open"), "{command}: {err}");
    }
}

#[test]
fn create_then_open_returns_config_and_tree_for_every_engine() {
    for (engine, main) in [
        ("typst", "main.typ"),
        ("latex", "main.tex"),
        ("markdown", "index.md"),
    ] {
        let h = harness();
        let dir = h.create(engine);
        let opened = h.ok("project.close", json!({}));
        assert_eq!(opened, Value::Null);
        let v = h.ok("project.open", json!({ "path": dir }));
        assert_eq!(v["engine"], engine);
        assert_eq!(v["main"], main);
        assert_eq!(v["name"], "Paper");
        assert_eq!(v["warnings"], json!([]));
        let names: Vec<&str> = v["tree"]["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .filter_map(|e| e["name"].as_str())
            .collect();
        assert!(names.contains(&"manang.toml") && names.contains(&main));
        assert_eq!(
            h.ok("project.config", json!({}))["project"]["engine"],
            engine
        );
    }
}

#[test]
fn open_reports_missing_and_invalid_projects() {
    let h = harness();
    let path = h.work.to_string_lossy().into_owned();
    let err = h
        .call("project.open", json!({ "path": path }))
        .expect_err("no toml");
    assert!(err.contains("not a Manang project"), "{err}");

    fs::write(
        h.work.join("manang.toml"),
        "[project]\nengine = \"typst\"\nmian = 1\n",
    )
    .expect("write");
    let err = h
        .call("project.open", json!({ "path": path }))
        .expect_err("bad");
    assert!(err.contains("did you mean `main`"), "{err}");

    let err = h
        .call("project.open", json!({ "path": h.work.join("nope") }))
        .expect_err("missing dir");
    assert!(err.contains("does not exist"), "{err}");
}

#[test]
fn open_warns_when_the_main_file_is_missing() {
    let h = harness();
    fs::write(
        h.work.join("manang.toml"),
        "[project]\nengine = \"typst\"\n",
    )
    .expect("w");
    let v = h.ok("project.open", json!({ "path": h.work }));
    assert_eq!(v["warnings"][0], "main file `main.typ` does not exist yet");
}

#[test]
fn file_commands_round_trip() {
    let h = harness();
    h.create("typst");
    assert_eq!(
        h.ok(
            "project.write-file",
            json!({ "path": "chapters/one.typ", "content": "= One\n" })
        ),
        json!({ "bytes": 6 })
    );
    assert_eq!(
        h.ok("project.read-file", json!({ "path": "chapters/one.typ" })),
        json!("= One\n")
    );
    h.ok(
        "project.rename",
        json!({ "from": "chapters/one.typ", "to": "chapters/uno.typ" }),
    );
    assert!(
        h.call("project.read-file", json!({ "path": "chapters/one.typ" }))
            .is_err()
    );
    let deleted = h.ok("project.delete", json!({ "path": "chapters" }));
    assert!(
        deleted["trashed_to"]
            .as_str()
            .is_some_and(|p| p.starts_with(".manang/trash/") && p.ends_with("/chapters"))
    );
    let tree = h.ok("project.tree", json!({ "recursive": true }));
    let paths: Vec<&str> = tree["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["path"].as_str())
        .collect();
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with("chapters") || p.starts_with(".manang"))
    );
}

#[test]
fn read_bytes_returns_a_binary_payload() {
    let h = harness();
    let dir = h.create("typst");
    fs::write(Path::new(&dir).join("fig.pdf"), [0u8, 159, 146, 150]).expect("w");
    match h.kernel.invoke(
        "project.read-bytes",
        Payload::Json(json!({ "path": "fig.pdf" })),
    ) {
        Ok(Payload::Binary(b)) => assert_eq!(&*b, &[0u8, 159, 146, 150]),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn escaping_paths_are_rejected_through_the_command_layer() {
    let h = harness();
    h.create("typst");
    fs::write(h.work.join("secret.txt"), "s").expect("w");
    for path in ["../secret.txt", "/etc/passwd", "a/../../secret.txt"] {
        assert!(
            h.call("project.read-file", json!({ "path": path }))
                .is_err(),
            "{path}"
        );
        assert!(
            h.call(
                "project.write-file",
                json!({ "path": path, "content": "x" })
            )
            .is_err(),
            "{path}"
        );
        assert!(
            h.call("project.delete", json!({ "path": path })).is_err(),
            "{path}"
        );
    }
    assert_eq!(
        fs::read_to_string(h.work.join("secret.txt")).expect("r"),
        "s"
    );
    let err = h
        .call(
            "project.write-file",
            json!({ "path": "x", "content": "", "extra": 1 }),
        )
        .expect_err("unknown arg");
    assert!(err.contains("unknown field"), "{err}");
}

#[test]
fn non_utf8_files_are_refused_as_text() {
    let h = harness();
    let dir = h.create("typst");
    fs::write(Path::new(&dir).join("b.bin"), [0xff, 0xfe]).expect("w");
    let err = h
        .call("project.read-file", json!({ "path": "b.bin" }))
        .expect_err("utf8");
    assert!(err.contains("not valid UTF-8"), "{err}");
}

#[test]
fn recent_lists_projects_most_recent_first_and_flags_missing_ones() {
    let h = harness();
    let first = h.work.join("one");
    let second = h.work.join("two");
    for (dir, name) in [(&first, "One"), (&second, "Two")] {
        h.ok(
            "project.create",
            json!({ "path": dir, "name": name, "engine": "markdown" }),
        );
    }
    let recent = h.ok("project.recent", json!({}));
    let names: Vec<&str> = recent
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|e| e["name"].as_str())
        .collect();
    assert_eq!(names, vec!["Two", "One"]);
    // Persisted as readable TOML in the config dir.
    let text = fs::read_to_string(h.config.join("recent-projects.toml")).expect("toml");
    assert!(text.contains("name = \"Two\""));
    // Reopening moves it to the front; deleting it flags `exists: false`.
    h.ok("project.open", json!({ "path": first }));
    fs::remove_dir_all(&second).expect("rm");
    let recent = h.ok("project.recent", json!({}));
    assert_eq!(recent[0]["name"], "One");
    assert_eq!(recent[0]["exists"], true);
    assert_eq!(recent[1]["exists"], false);
}

#[test]
fn watcher_publishes_debounced_changes_and_filters_ignored_paths() {
    let h = harness();
    let dir = h.create("typst");
    let sub = h.bus.subscribe("project.*");
    // Let the watcher settle, then discard scaffold noise.
    std::thread::sleep(Duration::from_millis(300));
    sub.drain();

    fs::create_dir_all(Path::new(&dir).join(".manang/cache")).expect("mk");
    fs::write(Path::new(&dir).join(".manang/cache/x"), "x").expect("w");
    h.ok(
        "project.write-file",
        json!({ "path": "notes.typ", "content": "hi" }),
    );

    let mut seen: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline && !seen.iter().any(|p| p == "notes.typ") {
        if let Some(event) = sub.recv_timeout(Duration::from_millis(200)) {
            assert_eq!(event.topic, "project.changed");
            let Payload::Json(v) = event.payload else {
                panic!("expected JSON");
            };
            for c in v["changes"].as_array().expect("changes") {
                seen.push(c["path"].as_str().expect("path").to_owned());
            }
        }
    }
    assert!(
        seen.iter().any(|p| p == "notes.typ"),
        "no event for notes.typ: {seen:?}"
    );
    assert!(!seen.iter().any(|p| p.starts_with(".manang")), "{seen:?}");
}

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn git_status_degrades_gracefully_outside_a_repo() {
    let h = harness();
    h.create("typst");
    let v = h.ok("project.git-status", json!({}));
    assert_eq!(v["is_repo"], false);
    assert_eq!(v["changes"], json!([]));
}

#[test]
fn git_status_reports_branch_and_changes() {
    if !git(Path::new("."), &["--version"]) {
        eprintln!("git binary not available; skipping");
        return;
    }
    let h = harness();
    let dir = h.create("typst");
    let dir = Path::new(&dir);
    assert!(git(dir, &["init", "-q", "-b", "trunk"]));

    let v = h.ok("project.git-status", json!({}));
    assert_eq!(v["is_repo"], true);
    assert_eq!(v["branch"], "trunk");
    assert_eq!(v["head"], Value::Null, "unborn branch has no head");
    assert!(
        v["changes"]
            .as_array()
            .expect("changes")
            .iter()
            .all(|c| c["unstaged"] == "untracked")
    );

    assert!(git(dir, &["add", "-A"]));
    assert!(git(dir, &["commit", "-q", "-m", "init"]));
    fs::write(dir.join("main.typ"), "changed\n").expect("w");
    fs::write(dir.join("new.typ"), "new\n").expect("w");
    fs::write(dir.join("staged.typ"), "s\n").expect("w");
    assert!(git(dir, &["add", "staged.typ"]));
    fs::remove_file(dir.join("references.bib")).expect("rm");

    let v = h.ok("project.git-status", json!({}));
    assert_eq!(v["branch"], "trunk");
    assert!(v["head"].as_str().is_some_and(|s| s.len() == 40));
    let find = |p: &str| {
        v["changes"]
            .as_array()
            .expect("changes")
            .iter()
            .find(|c| c["path"] == p)
            .cloned()
            .unwrap_or_else(|| panic!("{p} missing in {v}"))
    };
    assert_eq!(find("main.typ")["unstaged"], "modified");
    assert_eq!(find("new.typ")["unstaged"], "untracked");
    assert_eq!(find("staged.typ")["staged"], "added");
    assert_eq!(find("references.bib")["unstaged"], "deleted");
    assert_eq!(v["changes"].as_array().expect("changes").len(), 4);
}

#[test]
fn git_status_inside_a_subdirectory_of_a_repo_is_project_relative() {
    if !git(Path::new("."), &["--version"]) {
        return;
    }
    let h = harness();
    assert!(git(&h.work, &["init", "-q"]));
    let dir = h.create("typst"); // work/paper
    let v = h.ok("project.git-status", json!({}));
    assert_eq!(v["is_repo"], true);
    let paths: Vec<&str> = v["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .filter_map(|c| c["path"].as_str())
        .collect();
    assert!(paths.contains(&"main.typ"), "{paths:?} in {dir}");
    assert!(paths.iter().all(|p| !p.starts_with("paper/")));
}

#[test]
fn unknown_commands_are_reported_by_the_kernel() {
    let h = harness();
    assert!(matches!(
        h.kernel.invoke("project.nope", Payload::Empty),
        Err(KernelError::UnknownCommand(_))
    ));
}
