use manang_diagram::{manifest, plugin};
use manang_kernel::{Kernel, Payload};
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
        .join("../../testdata/robustness/diagram")
        .join(rel)
}

#[test]
fn test_empty_source_never_panics() {
    let k = test_kernel();

    // 1. Mermaid with empty source
    let res = k.invoke(
        "diagram.check",
        Payload::Json(json!({ "language": "mermaid", "source": "" })),
    );
    assert!(res.is_ok());
    if let Ok(Payload::Json(v)) = res {
        let diags = v["diagnostics"].as_array().unwrap();
        assert!(!diags.is_empty(), "expected diagnostic for empty mermaid");
        assert_eq!(diags[0]["line"], 1);
        assert_eq!(diags[0]["column"], 1);
    }

    // 2. Pikchr with empty source
    let res = k.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": "pikchr", "source": "" })),
    );
    assert!(res.is_ok(), "pikchr empty should succeed without panic");

    // 3. Svgbob with empty source
    let res = k.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": "svgbob", "source": "" })),
    );
    assert!(res.is_ok(), "svgbob empty should succeed without panic");
}

#[test]
fn test_unknown_language_and_unknown_diagram_type() {
    let k = test_kernel();

    // 1. Unknown language id
    let res = k.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": "nonexistent_lang", "source": "test" })),
    );
    assert!(res.is_err());
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("[unknown_language]"),
        "expected unknown_language error, got: {err}"
    );

    // 2. Unknown diagram type inside Mermaid
    let text = std::fs::read_to_string(testdata_path("unknown_type.mmd")).expect("read file");
    let res = k
        .invoke(
            "diagram.check",
            Payload::Json(json!({ "language": "mermaid", "source": text })),
        )
        .expect("check invoke");
    if let Payload::Json(v) = res {
        let diags = v["diagnostics"].as_array().unwrap();
        assert!(
            !diags.is_empty(),
            "expected diagnostics for unknown diagram type"
        );
        let d = &diags[0];
        assert!(d["line"].as_u64().unwrap() >= 1, "expected sensible line");
        assert!(
            d["column"].as_u64().unwrap() >= 1,
            "expected sensible column"
        );
    }
}

#[test]
fn test_unicode_and_rtl_labels() {
    let k = test_kernel();
    let text = std::fs::read_to_string(testdata_path("unicode_rtl.mmd")).expect("read file");

    let res = k
        .invoke(
            "diagram.render",
            Payload::Json(json!({ "language": "mermaid", "source": text, "theme": "dark" })),
        )
        .expect("render unicode mermaid");
    if let Payload::Binary(bytes) = res {
        let svg = String::from_utf8_lossy(&bytes);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("مرحبا"));
        assert!(svg.contains("שלום"));
    } else {
        panic!("expected binary payload");
    }
}

#[test]
fn test_html_and_markup_labels() {
    let k = test_kernel();
    let text = std::fs::read_to_string(testdata_path("html_labels.mmd")).expect("read file");

    let res = k
        .invoke(
            "diagram.render",
            Payload::Json(json!({ "language": "mermaid", "source": text, "theme": "print" })),
        )
        .expect("render html labels mermaid");
    if let Payload::Binary(bytes) = res {
        let svg = String::from_utf8_lossy(&bytes);
        println!("Rendered SVG with HTML labels:\n{svg}");
        assert!(svg.contains("<svg"));
    } else {
        panic!("expected binary payload");
    }
}

#[test]
fn test_pikchr_syntax_error_has_sensible_line_column() {
    let k = test_kernel();
    let text =
        std::fs::read_to_string(testdata_path("pikchr_syntax_error.pikchr")).expect("read file");

    let res = k
        .invoke(
            "diagram.check",
            Payload::Json(json!({ "language": "pikchr", "source": text })),
        )
        .expect("check pikchr");
    if let Payload::Json(v) = res {
        let diags = v["diagnostics"].as_array().unwrap();
        assert!(!diags.is_empty(), "expected syntax error in pikchr");
        let d = &diags[0];
        assert_eq!(d["line"], 3, "syntax error on line 3");
        assert!(d["column"].as_u64().unwrap() >= 1);
    }
}

#[test]
fn test_50000_character_source() {
    let k = test_kernel();

    // 50,000 characters in a flowchart
    let mut big_source = String::with_capacity(55000);
    big_source.push_str("flowchart TD\n");
    let long_label = "A".repeat(49900);
    big_source.push_str(&format!("  Node1[\"{long_label}\"] --> Node2\n"));

    let start = Instant::now();
    let res = k.invoke(
        "diagram.check",
        Payload::Json(json!({ "language": "mermaid", "source": big_source })),
    );
    let duration = start.elapsed();
    println!("Timing for 50,000-char Mermaid check: {:?}", duration);
    assert!(res.is_ok(), "50,000-char diagram check must not panic");
}

#[test]
fn test_2000_node_flowchart_performance_and_limit() {
    let k = test_kernel();

    // 2,000 nodes in flowchart
    let mut flowchart = String::with_capacity(2000 * 30);
    flowchart.push_str("flowchart TD\n");
    for i in 0..1999 {
        flowchart.push_str(&format!("  N{i} --> N{}\n", i + 1));
    }

    let start_check = Instant::now();
    let check_res = k.invoke(
        "diagram.check",
        Payload::Json(json!({ "language": "mermaid", "source": &flowchart })),
    );
    let check_duration = start_check.elapsed();
    println!(
        "Timing for 2,000-node flowchart check: {:?}",
        check_duration
    );
    assert!(check_res.is_ok(), "check must succeed without panic");

    let start_render = Instant::now();
    let render_res = k.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": "mermaid", "source": &flowchart })),
    );
    let render_duration = start_render.elapsed();
    println!(
        "Timing for 2,000-node flowchart render: {:?}",
        render_duration
    );

    // Must finish or return a limit error within 5 s
    assert!(
        render_duration.as_secs() < 5,
        "2,000-node flowchart render took {:?} (exceeded 5s SLA)",
        render_duration
    );

    match render_res {
        Ok(Payload::Binary(b)) => {
            let svg = String::from_utf8_lossy(&b);
            assert!(svg.contains("<svg"));
        }
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("limit") || err_str.contains("error"),
                "expected valid render or limit error, got: {err_str}"
            );
        }
        _ => panic!("unexpected response payload"),
    }
}
