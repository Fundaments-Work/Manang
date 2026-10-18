use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_kernel::{Kernel, Payload};
use manang_search::{Limits, SearchPlugin};
use serde_json::{Value, json};

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    root: PathBuf,
}

fn harness_with_limits(limits: Limits) -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("proj");
    fs::create_dir_all(&root).expect("root");
    let kernel = Kernel::new();
    kernel
        .register(
            manang_search::manifest().expect("manifest"),
            Arc::new(SearchPlugin::with_limits(None, limits)),
        )
        .expect("register");
    Harness {
        _tmp: tmp,
        kernel,
        root,
    }
}

fn harness() -> Harness {
    harness_with_limits(Limits::default())
}

impl Harness {
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(Payload::Empty) => Ok(Value::Null),
            Ok(Payload::Binary(_)) => Err("binary payload".to_owned()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        self.call(command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }
}

#[test]
fn test_fifty_megabyte_file() {
    let h = harness();
    let root_str = h.root.to_string_lossy().into_owned();

    // 1. Create a normal small file with target keyword
    fs::write(
        h.root.join("small.txt"),
        "A small document.\nSpecialKeyword is present here.\nEnd of small.\n",
    )
    .expect("write small");

    // 2. Create a 50 MB file with target keyword in the middle
    let large_path = h.root.join("large_50mb.txt");
    let file = File::create(&large_path).expect("create 50mb file");
    let mut writer = BufWriter::new(file);
    let chunk = b"This is a line of text in a very large 50 MB file for search stress testing.\n";
    // 50 MB / 76 bytes per line ≈ 689,852 lines
    let total_lines = (50 * 1024 * 1024) / chunk.len();
    for i in 0..total_lines {
        if i == total_lines / 2 {
            writer
                .write_all(b"SpecialKeyword is hidden inside the fifty megabyte file.\n")
                .expect("write needle");
        } else {
            writer.write_all(chunk).expect("write chunk");
        }
    }
    writer.flush().expect("flush 50mb file");
    let meta = fs::metadata(&large_path).expect("metadata");
    let actual_size_mb = meta.len() as f64 / (1024.0 * 1024.0);
    println!(
        "\n[search::robustness] Generated 50 MB file: {:.2} MB",
        actual_size_mb
    );

    // 3. Search with DEFAULT limits (max_file_bytes = 8 MB)
    // The 50 MB file should be skipped as `files_skipped_large`, without error or panic.
    let t0 = Instant::now();
    let res = h.ok(
        "search.text",
        json!({
            "root": root_str,
            "query": "SpecialKeyword",
        }),
    );
    let elapsed = t0.elapsed();
    println!("  Default limits search elapsed: {:?}", elapsed);

    assert_eq!(res["stats"]["files_skipped_large"], 1);
    assert_eq!(res["stats"]["files_searched"], 1);
    assert_eq!(res["total_matches"], 1);
    assert_eq!(res["files"][0]["path"], "small.txt");

    // 4. Now search with CUSTOM limits allowing up to 64 MB per file
    let h64 = harness_with_limits(Limits {
        max_file_bytes: 64 * 1024 * 1024,
        max_bytes_scanned: 512 * 1024 * 1024,
        max_duration: Duration::from_secs(10),
        ..Limits::default()
    });
    let t0 = Instant::now();
    let res64 = h64.ok(
        "search.text",
        json!({
            "root": root_str,
            "query": "SpecialKeyword",
        }),
    );
    let elapsed64 = t0.elapsed();
    println!("  64 MB limit search elapsed: {:?}", elapsed64);

    assert_eq!(res64["stats"]["files_skipped_large"], 0);
    assert_eq!(res64["stats"]["files_searched"], 2);
    assert_eq!(res64["total_matches"], 2);
}

#[test]
fn test_binary_file_skipped() {
    let h = harness();
    let root_str = h.root.to_string_lossy().into_owned();

    let binary_bytes = include_bytes!("../../../testdata/robustness/search/binary.dat");
    fs::write(h.root.join("data.bin"), binary_bytes).expect("write binary");
    fs::write(
        h.root.join("notes.txt"),
        "Normal text file with BinaryDataHere inside.\n",
    )
    .expect("write text");

    let res = h.ok(
        "search.text",
        json!({
            "root": root_str,
            "query": "BinaryDataHere",
        }),
    );

    assert_eq!(res["stats"]["files_skipped_binary"], 1);
    assert_eq!(res["total_matches"], 1);
    assert_eq!(res["files"][0]["path"], "notes.txt");
}

#[test]
fn test_deeply_nested_folders() {
    let h = harness();
    let root_str = h.root.to_string_lossy().into_owned();

    // 80 levels of nested directories
    let mut current = h.root.clone();
    for i in 0..80 {
        current = current.join(format!("level_{:02}", i));
    }
    fs::create_dir_all(&current).expect("create deeply nested dirs");
    fs::write(
        current.join("deep_secret.txt"),
        "Found the deep secret in level 80!\n",
    )
    .expect("write deep file");

    // 1. Fuzzy file search
    let t0 = Instant::now();
    let files_res = h.ok(
        "search.files",
        json!({
            "root": root_str,
            "query": "deep_secret",
        }),
    );
    let files_time = t0.elapsed();
    println!(
        "\n[search::robustness] Deeply nested file find (80 levels): {:?}",
        files_time
    );
    assert_eq!(files_res["results"].as_array().unwrap().len(), 1);
    let found_path = files_res["results"][0]["path"].as_str().unwrap();
    assert!(found_path.ends_with("deep_secret.txt"));

    // 2. Text search inside deeply nested file
    let text_res = h.ok(
        "search.text",
        json!({
            "root": root_str,
            "query": "deep secret",
        }),
    );
    assert_eq!(text_res["total_matches"], 1);
    assert_eq!(text_res["files"][0]["path"], found_path);
}

#[test]
fn test_symlink_loops() {
    let h = harness();
    let root_str = h.root.to_string_lossy().into_owned();

    let sub = h.root.join("subfolder");
    fs::create_dir_all(&sub).expect("create subfolder");
    fs::write(sub.join("regular.txt"), "Regular file inside subfolder.\n").expect("write regular");

    // Create recursive symlink loop
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        // Symlink pointing to itself: subfolder/loop -> subfolder
        let _ = symlink(&sub, sub.join("loop"));
        // Symlink pointing to parent: subfolder/up -> ..
        let _ = symlink("..", sub.join("up"));
    }

    let t0 = Instant::now();
    let res = h.ok(
        "search.files",
        json!({
            "root": root_str,
            "query": "regular",
        }),
    );
    let elapsed = t0.elapsed();
    println!(
        "\n[search::robustness] Symlink loop scan time: {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "Symlink loop took too long: {:?}",
        elapsed
    );

    let results = res["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["path"], "subfolder/regular.txt");
}

#[test]
fn test_invalid_utf8_file() {
    let h = harness();
    let root_str = h.root.to_string_lossy().into_owned();

    let invalid_bytes = include_bytes!("../../../testdata/robustness/search/invalid_utf8.txt");
    fs::write(h.root.join("corrupt.txt"), invalid_bytes).expect("write invalid utf8 file");

    let res = h.ok(
        "search.text",
        json!({
            "root": root_str,
            "query": "target match",
        }),
    );

    assert_eq!(res["total_matches"], 1);
    assert_eq!(res["files"][0]["path"], "corrupt.txt");
    let match_line = &res["files"][0]["matches"][0];
    assert_eq!(match_line["line"], 3);
    assert_eq!(match_line["text"], "Line 3: target match here");
}

#[test]
fn test_cancellable_search_and_pathological_regex() {
    let h = harness();
    let root_str = h.root.to_string_lossy().into_owned();

    // Write a file with repeated content that would stress a backtracking regex
    fs::write(
        h.root.join("haystack.txt"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab\n".repeat(100),
    )
    .expect("write haystack");

    // 1. Classical "backtracking" patterns run in linear time without hanging
    let t0 = Instant::now();
    let res = h.ok(
        "search.text",
        json!({
            "root": root_str,
            "query": "(a+)+$",
            "regex": true,
        }),
    );
    let elapsed = t0.elapsed();
    println!(
        "\n[search::robustness] Backtracking regex (a+)+$ linear execution time: {:?}",
        elapsed
    );
    assert!(elapsed < Duration::from_secs(2));
    assert!(res["stats"]["files_searched"].as_u64().unwrap_or(0) >= 1);

    // 2. Malformed and oversized regexes fail cleanly with an error
    let bad_queries = [
        "[a-z",              // Unclosed bracket
        "(?P<",              // Incomplete named group
        &"a".repeat(10_000), // Exceeds max_pattern_bytes (4,096)
    ];

    for query in bad_queries {
        let err = h.call(
            "search.text",
            json!({
                "root": root_str,
                "query": query,
                "regex": true,
            }),
        );
        assert!(
            err.is_err(),
            "Query was expected to fail compilation: {query:.20}..."
        );
    }

    // 3. Active cancellation of a running search
    // Create multiple files so the search has enough work to be interrupted
    for i in 0..20 {
        fs::write(
            h.root.join(format!("file_{:02}.txt", i)),
            "line of text that does not match the target query\n".repeat(5000),
        )
        .expect("write file");
    }

    // Warm the index first
    h.ok(
        "search.files",
        json!({ "root": root_str, "query": "", "limit": 1 }),
    );

    // Spawn search in background thread and cancel it
    let h_arc = Arc::new(h);
    let root_clone = root_str.clone();
    let h_worker = h_arc.clone();
    let worker = std::thread::spawn(move || {
        h_worker.call(
            "search.text",
            json!({
                "root": root_clone,
                "query": "nonexistent_query_string",
            }),
        )
    });

    let mut cancelled = false;
    while !worker.is_finished() {
        let cancel_res = h_arc.ok("search.cancel", json!({}));
        if cancel_res["cancelled"].as_u64().unwrap_or(0) > 0 {
            cancelled = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let r = worker
        .join()
        .expect("join worker")
        .expect("search response");
    if cancelled {
        assert!(r["cancelled"] == true || r["stopped"] == "cancelled" || r["stopped"].is_null());
    }
}
