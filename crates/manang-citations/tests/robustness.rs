use manang_citations::{MockHttpClient, manifest, plugin_with_client};
use manang_kernel::{Kernel, Payload};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn test_kernel() -> Kernel {
    let mock = Arc::new(MockHttpClient::new());
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(plugin_with_client(mock)),
        )
        .expect("registers");
    kernel
}

fn testdata_path(rel: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("../../testdata/robustness/citations")
        .join(rel)
}

#[test]
fn test_empty_bib_file() {
    let k = test_kernel();
    let path = testdata_path("empty.bib");
    let res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({ "path": path.to_str().unwrap() })),
        )
        .expect("invoke load");
    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(val["loaded"], 0);
    assert_eq!(val["total"], 0);
    assert!(val["errors"].as_array().unwrap().is_empty());
}

#[test]
fn test_corrupt_binary_file() {
    let k = test_kernel();
    let path = testdata_path("corrupt.bin");
    let res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({ "path": path.to_str().unwrap() })),
        )
        .expect("invoke load");
    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(val["loaded"], 0);
    let errors = val["errors"].as_array().unwrap();
    assert!(!errors.is_empty(), "expected error for binary file");
    assert_eq!(errors[0]["line"], 1);
}

#[test]
fn test_bom_bib_file() {
    let k = test_kernel();
    let path = testdata_path("with_bom.bib");
    let res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({ "path": path.to_str().unwrap() })),
        )
        .expect("invoke load");
    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(val["loaded"], 1, "BOM should be stripped and entry loaded");
    assert_eq!(val["errors"].as_array().unwrap().len(), 0);

    let entry = k
        .invoke("citations.get", Payload::Json(json!({ "key": "bom2024" })))
        .expect("get bom2024");
    if let Payload::Json(obj) = entry {
        assert_eq!(obj["title"], "BOM Test");
    } else {
        panic!("expected json payload");
    }
}

#[test]
fn test_bom_csl_json_file() {
    let k = test_kernel();
    let path = testdata_path("messy.json");
    let res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({ "path": path.to_str().unwrap() })),
        )
        .expect("invoke load");
    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(val["loaded"], 1, "CSL JSON with BOM should load");
    assert_eq!(val["errors"].as_array().unwrap().len(), 0);
}

#[test]
fn test_messy_bib_file_partial_results_and_warnings() {
    let k = test_kernel();
    let path = testdata_path("messy.bib");
    let res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({ "path": path.to_str().unwrap() })),
        )
        .expect("invoke load");
    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };

    println!(
        "messy.bib result: {}",
        serde_json::to_string_pretty(&val).unwrap()
    );
    // Expect partial results: at least recovered entries loaded
    assert!(
        val["loaded"].as_u64().unwrap() >= 3,
        "expected partial results, got loaded = {}",
        val["loaded"]
    );

    // Expect warnings/diagnostics for malformed entry or duplicate key
    let errors = val["errors"].as_array().unwrap();
    assert!(
        !errors.is_empty(),
        "expected diagnostics for malformed entry in messy.bib"
    );

    // Assert insta snapshot of diagnostics
    // The path differs per checkout, so it stays out of the snapshot.
    let mut errors = errors.clone();
    for error in &mut errors {
        if let Some(file) = error.get_mut("file") {
            *file = serde_json::Value::String("[path]/messy.bib".into());
        }
    }
    insta::assert_json_snapshot!("robustness_messy_bib_errors", errors);
}

#[test]
fn test_5000_entries_performance() {
    let k = test_kernel();
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let bib_path = temp_dir.path().join("large_5000.bib");

    let mut content = String::with_capacity(5000 * 180);
    for i in 0..5000 {
        use std::fmt::Write;
        let _ = writeln!(
            content,
            "@article{{entry_{i},\n  author = {{Author Number {i} and Coauthor}},\n  title = {{Paper Title For Benchmarking Entry {i}}},\n  journal = {{Journal of Performance}},\n  year = {},\n  pages = {{100--120}}\n}}",
            2000 + (i % 25)
        );
    }
    std::fs::write(&bib_path, &content).expect("write large bib");

    let start = Instant::now();
    let res = k
        .invoke(
            "citations.load",
            Payload::Json(json!({ "path": bib_path.to_str().unwrap() })),
        )
        .expect("invoke load");
    let duration = start.elapsed();
    println!("Timing for 5,000 BibTeX entries: {:?}", duration);

    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json payload"),
    };
    assert_eq!(val["loaded"], 5000);
    assert_eq!(val["errors"].as_array().unwrap().len(), 0);
    assert!(
        duration.as_millis() < 500,
        "5,000 entries took {:?} (expected < 500 ms)",
        duration
    );
}
