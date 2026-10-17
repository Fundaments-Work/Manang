//! Every `search.*` command driven through the real kernel on temp dirs.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{EventBus, Kernel, Payload};
use manang_search::{Limits, SearchPlugin};
use serde_json::{Value, json};

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    root: PathBuf,
}

fn harness_with(events: Option<EventBus>, limits: Limits) -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("proj");
    fs::create_dir_all(&root).expect("root");
    let kernel = Kernel::new();
    kernel
        .register(
            manang_search::manifest().expect("manifest"),
            Arc::new(SearchPlugin::with_limits(events, limits)),
        )
        .expect("register");
    Harness {
        _tmp: tmp,
        kernel,
        root,
    }
}

fn harness() -> Harness {
    harness_with(None, Limits::default())
}

fn write(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(p, content).expect("write");
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

    fn root_str(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn text(&self, query: &str, extra: Value) -> Value {
        let mut args = json!({ "root": self.root_str(), "query": query });
        if let (Value::Object(a), Value::Object(b)) = (&mut args, extra) {
            a.extend(b);
        }
        self.ok("search.text", args)
    }

    fn file_paths(&self, query: &str, extra: Value) -> Vec<String> {
        let mut args = json!({ "root": self.root_str(), "query": query });
        if let (Value::Object(a), Value::Object(b)) = (&mut args, extra) {
            a.extend(b);
        }
        self.ok("search.files", args)["results"]
            .as_array()
            .expect("results")
            .iter()
            .map(|r| r["path"].as_str().expect("path").to_owned())
            .collect()
    }
}

fn matched_paths(v: &Value) -> Vec<String> {
    v["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().expect("path").to_owned())
        .collect()
}

fn project(h: &Harness) {
    write(&h.root, ".gitignore", "build/\n*.log\n");
    write(&h.root, "main.typ", "= Title\nhello world\nHello again\n");
    write(&h.root, "src/lib.rs", "fn hello() {}\n// world\n");
    write(&h.root, "build/out.typ", "hello hidden by gitignore\n");
    write(&h.root, "debug.log", "hello log\n");
    write(&h.root, ".hidden/secret.txt", "hello secret\n");
    write(&h.root, ".git/config", "hello git\n");
    write(&h.root, "bin.dat", b"hello\0binary");
}

#[test]
fn manifest_is_native_search() {
    let m = manang_search::manifest().expect("manifest");
    assert_eq!(m.plugin.name, "search");
    assert_eq!(m.contributes.commands.len(), 6);
}

#[test]
fn files_respects_gitignore_and_hidden_option() {
    let h = harness();
    project(&h);
    let all = h.file_paths("", json!({ "limit": 100 }));
    assert!(all.contains(&"main.typ".to_owned()));
    assert!(all.contains(&"src/lib.rs".to_owned()));
    assert!(!all.iter().any(|p| p.starts_with("build/")), "{all:?}");
    assert!(!all.contains(&"debug.log".to_owned()));
    assert!(!all.iter().any(|p| p.starts_with(".hidden")));
    assert!(!all.iter().any(|p| p.starts_with(".git/")));

    let with_hidden = h.file_paths("", json!({ "limit": 100, "hidden": true }));
    assert!(with_hidden.contains(&".hidden/secret.txt".to_owned()));
    assert!(!with_hidden.iter().any(|p| p.starts_with(".git/")));
}

#[test]
fn files_ranks_and_highlights() {
    let h = harness();
    write(&h.root, "docs/maintenance/notes.md", "x");
    write(&h.root, "src/main.rs", "x");
    let r = h.ok(
        "search.files",
        json!({ "root": h.root_str(), "query": "main" }),
    );
    let first = &r["results"][0];
    assert_eq!(first["path"], "src/main.rs");
    assert_eq!(first["indices"], json!([4, 5, 6, 7]));
    assert_eq!(first["name_start"], 4);
    assert_eq!(r["total_indexed"], 2);
}

#[test]
fn files_index_is_invalidated_by_project_changed() {
    let bus = EventBus::new();
    let h = harness_with(Some(bus.clone()), Limits::default());
    write(&h.root, "a.typ", "x");
    assert_eq!(h.file_paths("", json!({})), vec!["a.typ".to_owned()]);
    write(&h.root, "b.typ", "x");
    // The bus-driven cache trusts events, so the new file is not seen yet.
    assert_eq!(h.file_paths("", json!({})).len(), 1);

    let canonical = h.root.canonicalize().expect("canon");
    bus.emit(
        "project.changed",
        Payload::Json(json!({
            "root": canonical.to_string_lossy(),
            "changes": [{ "path": "b.typ", "kind": "created" }],
        })),
    );
    assert_eq!(h.file_paths("", json!({})).len(), 2);

    // Plain modifications do not rebuild the index.
    write(&h.root, "c.typ", "x");
    bus.emit(
        "project.changed",
        Payload::Json(json!({
            "root": canonical.to_string_lossy(),
            "changes": [{ "path": "a.typ", "kind": "modified" }],
        })),
    );
    assert_eq!(h.file_paths("", json!({})).len(), 2);
    // ...but an edited .gitignore does.
    bus.emit(
        "project.changed",
        Payload::Json(json!({
            "root": canonical.to_string_lossy(),
            "changes": [{ "path": ".gitignore", "kind": "modified" }],
        })),
    );
    assert_eq!(h.file_paths("", json!({})).len(), 3);
}

#[test]
fn files_without_bus_expires_by_ttl() {
    let h = harness();
    write(&h.root, "a.typ", "x");
    assert_eq!(h.file_paths("", json!({})).len(), 1);
    write(&h.root, "b.typ", "x");
    std::thread::sleep(manang_search::NO_BUS_TTL + Duration::from_millis(200));
    assert_eq!(h.file_paths("", json!({})).len(), 2);
}

#[test]
fn text_respects_gitignore_and_skips_binary_and_internal() {
    let h = harness();
    project(&h);
    let r = h.text("hello", json!({ "limit": 100 }));
    let paths = matched_paths(&r);
    assert_eq!(paths, vec!["main.typ", "src/lib.rs"]);
    assert_eq!(r["total_matches"], 3);
    assert_eq!(r["truncated"], false);
    assert!(r["id"].as_u64().is_some());

    let r = h.text("hello", json!({ "hidden": true }));
    assert!(matched_paths(&r).contains(&".hidden/secret.txt".to_owned()));
    // The binary file is never listed, in either mode.
    assert!(!matched_paths(&r).contains(&"bin.dat".to_owned()));
    assert_eq!(r["stats"]["files_skipped_binary"], 1);
}

#[test]
fn text_case_word_and_regex_options() {
    let h = harness();
    write(&h.root, "a.txt", "Cat concat cat\ncat.\n");
    write(&h.root, "b.txt", "a.b a+b\n");
    let n = |extra: Value| h.text("cat", extra)["total_ranges"].as_u64().expect("n");
    assert_eq!(n(json!({})), 4);
    assert_eq!(n(json!({ "case_sensitive": true })), 3);
    assert_eq!(n(json!({ "whole_word": true })), 3);
    let r = h.text(r"c\w+t", json!({ "regex": true }));
    assert_eq!(r["total_ranges"], 4);
    // A literal query is not a regex.
    let r = h.text("a.b", json!({ "include": ["b.txt"] }));
    assert_eq!(r["total_ranges"], 1);
}

#[test]
fn text_include_exclude_globs() {
    let h = harness();
    write(&h.root, "a.rs", "needle\n");
    write(&h.root, "b.md", "needle\n");
    write(&h.root, "sub/c.rs", "needle\n");
    write(&h.root, "vendor/d.rs", "needle\n");
    let r = h.text(
        "needle",
        json!({ "include": ["*.rs"], "exclude": ["vendor"] }),
    );
    assert_eq!(matched_paths(&r), vec!["a.rs", "sub/c.rs"]);
    let r = h.text("needle", json!({ "include": ["sub/"] }));
    assert_eq!(matched_paths(&r), vec!["sub/c.rs"]);
}

#[test]
fn text_unicode_offsets_are_scalar_indices() {
    let h = harness();
    // Line 2: emoji (1 scalar, 2 UTF-16 units), 'e' + combining acute, then
    // the needle at scalar index 6.
    write(&h.root, "u.txt", "ascii\n😀 e\u{301} 🎉needle\n");
    let r = h.text("needle", json!({}));
    let m = &r["files"][0]["matches"][0];
    assert_eq!(m["line"], 2);
    assert_eq!(m["ranges"], json!([{ "start": 6, "end": 12 }]));
    assert_eq!(m["column"], 7);
    // "ascii\n" is 6 bytes; the line start is byte 6.
    assert_eq!(m["byte_offset"], 6);
    // Searching the combining mark alone lands on its own scalar index.
    let r = h.text("\u{301}", json!({}));
    assert_eq!(
        r["files"][0]["matches"][0]["ranges"],
        json!([{ "start": 3, "end": 4 }])
    );
}

#[test]
fn text_crlf_and_context_lines() {
    let h = harness();
    write(&h.root, "c.txt", "one\r\ntwo\r\nneedle\r\nfour\r\nfive\r\n");
    let r = h.text("needle$", json!({ "regex": true, "context_lines": 1 }));
    let m = &r["files"][0]["matches"][0];
    assert_eq!(m["text"], "needle");
    assert_eq!(m["line"], 3);
    assert_eq!(m["before"], json!([{ "line": 2, "text": "two" }]));
    assert_eq!(m["after"], json!([{ "line": 4, "text": "four" }]));
}

#[test]
fn text_result_cap_and_limit() {
    let h = harness();
    for i in 0..20 {
        write(&h.root, &format!("f{i:02}.txt"), "x\nx\nx\n");
    }
    let r = h.text("x", json!({ "limit": 7 }));
    assert_eq!(r["total_matches"], 7);
    assert_eq!(r["truncated"], true);
    assert_eq!(r["stopped"], "limit");
}

#[test]
fn text_caps_on_bytes_time_and_file_size() {
    let limits = Limits {
        max_bytes_scanned: 100,
        ..Limits::default()
    };
    let h = harness_with(None, limits);
    write(&h.root, "a.txt", "x".repeat(80));
    write(&h.root, "b.txt", "x".repeat(80));
    let r = h.text("x", json!({}));
    assert_eq!(r["stopped"], "bytes");
    assert!(r["stats"]["bytes_scanned"].as_u64().expect("b") <= 200);

    let limits = Limits {
        max_duration: Duration::ZERO,
        ..Limits::default()
    };
    let h = harness_with(None, limits);
    write(&h.root, "a.txt", "x");
    let r = h.text("x", json!({}));
    assert_eq!(r["stopped"], "time");
    assert_eq!(r["truncated"], true);

    let limits = Limits {
        max_file_bytes: 10,
        ..Limits::default()
    };
    let h = harness_with(None, limits);
    write(&h.root, "big.txt", "x".repeat(100));
    write(&h.root, "small.txt", "x");
    let r = h.text("x", json!({}));
    assert_eq!(matched_paths(&r), vec!["small.txt"]);
    assert_eq!(r["stats"]["files_skipped_large"], 1);
}

#[test]
fn text_overlong_lines_are_windowed() {
    let h = harness();
    let line = format!("{}needle{}", "a".repeat(5000), "b".repeat(5000));
    write(&h.root, "min.js", line);
    let r = h.text("needle", json!({}));
    let m = &r["files"][0]["matches"][0];
    let text = m["text"].as_str().expect("text");
    assert!(text.chars().count() <= 1000);
    let off = m["text_offset"].as_u64().expect("offset") as usize;
    let s = m["ranges"][0]["start"].as_u64().expect("s") as usize;
    assert_eq!(&text.chars().skip(s).take(6).collect::<String>(), "needle");
    assert_eq!(m["column"], json!(off + s + 1));
}

#[test]
fn pathological_and_invalid_patterns_error_quickly() {
    let h = harness();
    write(&h.root, "a.txt", "aaaa\n");
    for q in ["((a{1000}){1000}){1000}", "(", "a\\nb", "(?<=a)b", ""] {
        let started = std::time::Instant::now();
        let r = h.call(
            "search.text",
            json!({ "root": h.root_str(), "query": q, "regex": true }),
        );
        assert!(r.is_err(), "{q:?} should be refused");
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    // Catastrophic-backtracking shape is linear-time on this engine.
    write(&h.root, "b.txt", format!("{}!\n", "a".repeat(5000)));
    let started = std::time::Instant::now();
    h.text("(a+)+$", json!({ "regex": true }));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn cancel_with_nothing_running_and_ids_increase() {
    let h = harness();
    write(&h.root, "a.txt", "x");
    assert_eq!(h.ok("search.cancel", json!({}))["cancelled"], 0);
    assert_eq!(h.ok("search.cancel", json!({ "id": 99 }))["cancelled"], 0);
    let a = h.text("x", json!({}))["id"].as_u64().expect("id");
    let b = h.text("x", json!({}))["id"].as_u64().expect("id");
    assert!(b > a);
}

#[test]
fn cancel_stops_a_running_search() {
    let h = Arc::new(harness());
    for d in 0..40 {
        for f in 0..50 {
            write(&h.root, &format!("d{d}/f{f}.txt"), "line\n".repeat(2000));
        }
    }
    // Warm the index so the search itself is what runs while we cancel.
    h.file_paths("", json!({ "limit": 1 }));
    let worker = {
        let h = h.clone();
        std::thread::spawn(move || h.text("zzz_not_present_zzz", json!({})))
    };
    let mut cancelled_any = false;
    while !worker.is_finished() {
        if h.ok("search.cancel", json!({}))["cancelled"].as_u64() > Some(0) {
            cancelled_any = true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let r = worker.join().expect("worker");
    if cancelled_any {
        // Either the cancel landed mid-search (reported) or the search had
        // just finished; both are valid, a hang or panic is not.
        assert!(r["cancelled"] == true || r["stopped"].is_null());
    }
}

#[test]
fn replace_preview_does_not_touch_disk_and_apply_is_atomic() {
    let h = harness();
    write(&h.root, "a.txt", "foo bar foo\nnext foo\n");
    write(&h.root, "b.txt", "nothing\n");
    let before = fs::read(h.root.join("a.txt")).expect("read");
    let p = h.ok(
        "search.replace-preview",
        json!({ "root": h.root_str(), "query": "foo", "replacement": "baz" }),
    );
    assert_eq!(fs::read(h.root.join("a.txt")).expect("read"), before);
    assert_eq!(p["total_edits"], 3);
    assert_eq!(p["files"].as_array().expect("f").len(), 1);
    let e = &p["files"][0]["edits"][2];
    assert_eq!(
        (e["line"].as_u64(), e["start"].as_u64()),
        (Some(2), Some(5))
    );
    assert_eq!(e["offset_start"], 17);
    assert_eq!(e["matched"], "foo");

    let plan_id = p["plan_id"].as_str().expect("plan").to_owned();
    let a = h.ok("search.replace-apply", json!({ "plan_id": plan_id }));
    assert_eq!(a["applied"], 1);
    assert_eq!(a["failed"], 0);
    assert_eq!(
        fs::read_to_string(h.root.join("a.txt")).expect("read"),
        "baz bar baz\nnext baz\n"
    );
    // No temp files left behind.
    let leftovers: Vec<_> = fs::read_dir(&h.root)
        .expect("dir")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(".manang-tmp"))
        .collect();
    assert!(leftovers.is_empty());
    // Plans are single use.
    assert!(
        h.call("search.replace-apply", json!({ "plan_id": plan_id }))
            .is_err()
    );
}

#[test]
fn replace_apply_refuses_files_changed_since_preview() {
    let h = harness();
    write(&h.root, "a.txt", "foo\n");
    write(&h.root, "b.txt", "foo\n");
    let p = h.ok(
        "search.replace-preview",
        json!({ "root": h.root_str(), "query": "foo", "replacement": "bar" }),
    );
    write(&h.root, "a.txt", "foo edited meanwhile\n");
    let a = h.ok("search.replace-apply", json!({ "plan_id": p["plan_id"] }));
    assert_eq!(a["applied"], 1);
    assert_eq!(a["failed"], 1);
    let files = a["files"].as_array().expect("files");
    let fa = files.iter().find(|f| f["path"] == "a.txt").expect("a");
    assert_eq!(fa["ok"], false);
    assert!(fa["error"].as_str().expect("err").contains("changed"));
    assert_eq!(
        fs::read_to_string(h.root.join("a.txt")).expect("read"),
        "foo edited meanwhile\n"
    );
    assert_eq!(
        fs::read_to_string(h.root.join("b.txt")).expect("read"),
        "bar\n"
    );
}

#[test]
fn replace_regex_captures_unicode_and_subset() {
    let h = harness();
    write(&h.root, "a.txt", "😀 user@host x\n");
    write(&h.root, "b.txt", "other@place\n");
    let p = h.ok(
        "search.replace-preview",
        json!({
            "root": h.root_str(), "query": r"(\w+)@(\w+)", "regex": true,
            "replacement": "$2 at $1",
        }),
    );
    let e = &p["files"][0]["edits"][0];
    assert_eq!(e["replacement"], "host at user");
    assert_eq!(
        (e["start"].as_u64(), e["end"].as_u64()),
        (Some(2), Some(11))
    );
    let a = h.ok(
        "search.replace-apply",
        json!({ "plan_id": p["plan_id"], "files": ["a.txt"] }),
    );
    assert_eq!(a["applied"], 1);
    assert_eq!(a["files"].as_array().expect("f").len(), 1);
    assert_eq!(
        fs::read_to_string(h.root.join("a.txt")).expect("read"),
        "😀 host at user x\n"
    );
    assert_eq!(
        fs::read_to_string(h.root.join("b.txt")).expect("read"),
        "other@place\n"
    );
}

#[test]
fn replace_literal_does_not_expand_dollars_and_skips_binary() {
    let h = harness();
    write(&h.root, "a.txt", "price\n");
    write(&h.root, "bin.dat", b"price\0\n");
    let p = h.ok(
        "search.replace-preview",
        json!({ "root": h.root_str(), "query": "price", "replacement": "$1 and $$" }),
    );
    assert_eq!(p["files"].as_array().expect("f").len(), 1);
    h.ok("search.replace-apply", json!({ "plan_id": p["plan_id"] }));
    assert_eq!(
        fs::read_to_string(h.root.join("a.txt")).expect("read"),
        "$1 and $$\n"
    );
    assert_eq!(
        fs::read(h.root.join("bin.dat")).expect("read"),
        b"price\0\n"
    );
}

#[test]
fn replace_apply_confines_paths_to_the_root() {
    use manang_project::ProjectRoot;
    use manang_search::replace::{EditRec, PlanFile, PlanStore, apply};

    let h = harness();
    let outside = h.root.parent().expect("parent").join("outside.txt");
    fs::write(&outside, "foo").expect("outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, h.root.join("link.txt")).expect("symlink");
    write(&h.root, ".git/config", "foo");

    let root = ProjectRoot::open(&h.root).expect("root");
    let edit = |p: &str| PlanFile {
        path: p.to_owned(),
        hash: blake3_of(b"foo"),
        edits: vec![EditRec {
            byte_start: 0,
            byte_end: 3,
            replacement: "PWNED".to_owned(),
        }],
    };
    let store = PlanStore::default();
    let id = store.insert(
        root,
        vec![
            edit("../outside.txt"),
            edit("link.txt"),
            edit(".git/config"),
        ],
    );
    let plan = store.take(&id).expect("plan");
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("pool");
    let results = apply(&pool, &plan, None, &Limits::default());
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| !r.ok), "{results:?}");
    assert_eq!(fs::read_to_string(&outside).expect("read"), "foo");
    assert_eq!(
        fs::read_to_string(h.root.join(".git/config")).expect("read"),
        "foo"
    );
}

fn blake3_of(bytes: &[u8]) -> String {
    // Mirror of the plan hash without exposing a helper from the crate.
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

#[test]
fn replace_unknown_plan_and_oversized_replacement() {
    let h = harness();
    assert!(
        h.call("search.replace-apply", json!({ "plan_id": "nope" }))
            .is_err()
    );
    write(&h.root, "a.txt", "x");
    let big = "y".repeat(Limits::default().max_replacement_bytes + 1);
    assert!(
        h.call(
            "search.replace-preview",
            json!({ "root": h.root_str(), "query": "x", "replacement": big })
        )
        .is_err()
    );
}

#[test]
fn in_text_returns_scalar_ranges() {
    let h = harness();
    let r = h.ok(
        "search.in-text",
        json!({ "text": "😀 Foo\nfoo e\u{301}foo", "query": "foo" }),
    );
    let got: Vec<(u64, u64, u64, u64)> = r["matches"]
        .as_array()
        .expect("m")
        .iter()
        .map(|m| {
            (
                m["start"].as_u64().expect("s"),
                m["end"].as_u64().expect("e"),
                m["line"].as_u64().expect("l"),
                m["column"].as_u64().expect("c"),
            )
        })
        .collect();
    assert_eq!(got, vec![(2, 5, 1, 3), (6, 9, 2, 1), (12, 15, 2, 7)]);
    assert_eq!(r["truncated"], false);

    let r = h.ok(
        "search.in-text",
        json!({ "text": "Foo foo", "query": "foo", "case_sensitive": true }),
    );
    assert_eq!(r["matches"].as_array().expect("m").len(), 1);
    assert!(
        h.call(
            "search.in-text",
            json!({ "text": "x", "query": "(", "regex": true })
        )
        .is_err()
    );
}

#[test]
fn bad_arguments_are_rejected() {
    let h = harness();
    assert!(h.call("search.files", json!({ "query": "x" })).is_err());
    assert!(
        h.call(
            "search.text",
            json!({ "root": h.root_str(), "query": "x", "bogus": 1 })
        )
        .is_err()
    );
    assert!(
        h.call(
            "search.text",
            json!({ "root": "/definitely/not/here", "query": "x" })
        )
        .is_err()
    );
}

#[test]
fn text_result_snapshot() {
    let h = harness();
    project(&h);
    let mut r = h.text("hello", json!({ "context_lines": 1 }));
    if let Value::Object(m) = &mut r {
        m.remove("id");
        if let Some(Value::Object(s)) = m.get_mut("stats") {
            s.remove("elapsed_ms");
        }
    }
    insta::assert_snapshot!(serde_json::to_string_pretty(&r).expect("json"));
}
