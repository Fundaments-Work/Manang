use manang_kernel::{Kernel, Payload};
use manang_markdown::{manifest, plugin};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn test_kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest"), Arc::new(plugin()))
        .expect("registers");
    kernel
}

fn testdata_path(rel: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("../../testdata/robustness/markdown")
        .join(rel)
}

#[test]
fn test_unclosed_fences_never_panic() {
    let k = test_kernel();
    let text = "# Section\n\n```rust\nfn broken() {\n";

    // 1. render-html
    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": text })),
        )
        .expect("render-html");
    if let Payload::Json(v) = res {
        assert!(v["html"].as_str().unwrap().contains("<pre><code"));
    } else {
        panic!("expected json payload");
    }

    // 2. outline
    let res = k
        .invoke("markdown.outline", Payload::Json(json!({ "source": text })))
        .expect("outline");
    if let Payload::Json(v) = res {
        assert_eq!(v["headings"].as_array().unwrap().len(), 1);
    } else {
        panic!("expected json payload");
    }

    // 3. to-typst
    let res = k
        .invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": text })),
        )
        .expect("to-typst");
    if let Payload::Json(v) = res {
        assert!(v["typst"].as_str().unwrap().contains("```rust"));
    } else {
        panic!("expected json payload");
    }
}

#[test]
fn test_deeply_nested_lists_and_blockquotes_1000_levels() {
    let k = test_kernel();

    // 1,000 levels of blockquotes: > > > ...
    let mut deep_quotes = String::with_capacity(3000);
    for _ in 0..1000 {
        deep_quotes.push_str("> ");
    }
    deep_quotes.push_str("Deep Quote Text\n");

    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": deep_quotes })),
        )
        .expect("render deep quotes");
    if let Payload::Json(v) = res {
        assert!(v["html"].as_str().unwrap().contains("Deep Quote Text"));
    }

    let res = k
        .invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": deep_quotes })),
        )
        .expect("to-typst deep quotes");
    if let Payload::Json(v) = res {
        assert!(v["typst"].as_str().unwrap().contains("#quote"));
    }

    // 1,000 levels of nested lists
    let mut deep_lists = String::with_capacity(2000 * 1000);
    for i in 0..1000 {
        deep_lists.push_str(&"  ".repeat(i));
        deep_lists.push_str("- Item\n");
    }

    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": deep_lists })),
        )
        .expect("render deep lists");
    if let Payload::Json(v) = res {
        assert!(v["html"].as_str().unwrap().contains("Item"));
    }

    let res = k
        .invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": deep_lists })),
        )
        .expect("to-typst deep lists");
    if let Payload::Json(v) = res {
        assert!(v["typst"].as_str().unwrap().contains("- Item"));
    }
}

#[test]
fn test_huge_table() {
    let k = test_kernel();

    // 1,000 rows by 20 columns
    let mut table = String::with_capacity(1000 * 20 * 12);
    // Header
    table.push('|');
    for col in 0..20 {
        table.push_str(&format!(" Col {col} |"));
    }
    table.push('\n');
    // Separator
    table.push('|');
    for _ in 0..20 {
        table.push_str(" --- |");
    }
    table.push('\n');
    // Rows
    for row in 0..1000 {
        table.push('|');
        for col in 0..20 {
            table.push_str(&format!(" r{row}c{col} |"));
        }
        table.push('\n');
    }

    let start = Instant::now();
    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": table })),
        )
        .expect("render huge table");
    let duration = start.elapsed();
    println!("Timing for 1,000-row table HTML render: {:?}", duration);

    if let Payload::Json(v) = res {
        let html = v["html"].as_str().unwrap();
        assert!(html.contains("<table>"));
        assert!(html.contains("r999c19"));
    } else {
        panic!("expected json payload");
    }

    let res_typst = k
        .invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": table })),
        )
        .expect("typst huge table");
    if let Payload::Json(v) = res_typst {
        let typ = v["typst"].as_str().unwrap();
        assert!(typ.contains("#table(\n  columns: 20,"));
    }
}

#[test]
fn test_raw_html_scripts_and_event_handlers_sanitized() {
    let k = test_kernel();
    let text = std::fs::read_to_string(testdata_path("messy.md")).expect("read messy.md");

    // 1. Without allow_raw_html (default)
    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": text })),
        )
        .expect("render messy.md default");
    let val = match res {
        Payload::Json(v) => v,
        _ => panic!("expected json"),
    };
    let html = val["html"].as_str().unwrap();

    // Must NEVER contain raw executable script tags or elements
    assert!(!html.contains("<script>"), "raw script tag leaked!");
    assert!(html.contains("&lt;script&gt;"), "script must be escaped");
    assert!(!html.contains("<img"), "raw img leaked!");
    assert!(
        !html.contains("<span on"),
        "raw span with event handler leaked!"
    );

    // Must NEVER contain javascript: or data: link destinations
    assert!(
        !html.contains("href=\"javascript:"),
        "javascript url leaked!"
    );
    assert!(!html.contains("href=\"data:"), "data url leaked!");

    // 2. With allow_raw_html = true
    let res_allow = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({
                "source": text,
                "allow_raw_html": true
            })),
        )
        .expect("render messy.md with allow_raw_html");
    let val_allow = match res_allow {
        Payload::Json(v) => v,
        _ => panic!("expected json"),
    };
    let html_allow = val_allow["html"].as_str().unwrap();

    // Even with allow_raw_html = true, scripts and event handlers MUST be sanitized!
    assert!(!html_allow.contains("<script>"), "script tag permitted!");
    assert!(!html_allow.contains("<img"), "img tag permitted!");
    assert!(
        !html_allow.contains("<span on"),
        "span with event handler permitted!"
    );
    assert!(
        !html_allow.contains("href=\"javascript:"),
        "javascript url permitted!"
    );
    assert!(!html_allow.contains("href=\"data:"), "data url permitted!");

    // Assert insta snapshot of diagnostics for messy.md
    let diags = val["diagnostics"].as_array().unwrap();
    assert!(
        !diags.is_empty(),
        "expected diagnostics for sanitized items"
    );
    insta::assert_json_snapshot!("robustness_markdown_messy_diagnostics", diags);
}

#[test]
fn test_giant_single_line_5mb() {
    let k = test_kernel();

    // 5 MB of repeating words on a single line with headings
    let word = "word ";
    let count = (5 * 1024 * 1024) / word.len();
    let mut giant = String::with_capacity(count * word.len() + 100);
    giant.push_str("# Giant Header ");
    for _ in 0..count {
        giant.push_str(word);
    }

    let start = Instant::now();
    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": giant })),
        )
        .expect("render 5mb line");
    let duration = start.elapsed();
    println!("Timing for 5 MB single-line HTML render: {:?}", duration);

    if let Payload::Json(v) = res {
        assert!(v["html"].as_str().unwrap().contains("Giant Header"));
    }

    let start_outline = Instant::now();
    let res_outline = k
        .invoke(
            "markdown.outline",
            Payload::Json(json!({ "source": giant })),
        )
        .expect("outline 5mb line");
    println!(
        "Timing for 5 MB single-line outline: {:?}",
        start_outline.elapsed()
    );
    if let Payload::Json(v) = res_outline {
        assert_eq!(v["headings"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn test_unbalanced_math_formulas() {
    let k = test_kernel();
    let text = std::fs::read_to_string(testdata_path("unbalanced_math.md")).expect("read file");

    let res = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": text })),
        )
        .expect("render unbalanced math");
    if let Payload::Json(v) = res {
        let html = v["html"].as_str().unwrap();
        assert!(html.contains("price"));
    }

    let res_typ = k
        .invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": text })),
        )
        .expect("to-typst unbalanced math");
    if let Payload::Json(v) = res_typ {
        assert!(v["typst"].as_str().unwrap().contains("price"));
    }
}

#[test]
fn test_mermaid_fences_with_errors() {
    let k = test_kernel();
    let text = std::fs::read_to_string(testdata_path("mermaid_error.md")).expect("read file");

    let res_html = k
        .invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": text })),
        )
        .expect("render html mermaid error");
    if let Payload::Json(v) = res_html {
        let html = v["html"].as_str().unwrap();
        assert!(html.contains("<figure class=\"mng-diagram\""));
        assert_eq!(v["diagrams"].as_array().unwrap().len(), 1);
    }

    let res_diagrams = k
        .invoke(
            "markdown.diagrams",
            Payload::Json(json!({ "source": text })),
        )
        .expect("diagrams command");
    if let Payload::Json(v) = res_diagrams {
        let diagrams = v["diagrams"].as_array().unwrap();
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0]["language"], "mermaid");
    }

    let res_typ = k
        .invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": text })),
        )
        .expect("to typst");
    if let Payload::Json(v) = res_typ {
        assert!(v["typst"].as_str().unwrap().contains("```mermaid"));
    }
}
