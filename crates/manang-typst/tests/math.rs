//! `typst.math-to-svg`: one formula in, a vector SVG out, through the kernel.

use std::sync::Arc;

use manang_kernel::{Kernel, Payload};
use manang_typst::{CompilerConfig, manifest, plugin};
use serde_json::{Value, json};

fn kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(plugin(CompilerConfig::default()).expect("plugin starts")),
        )
        .expect("registers");
    kernel
}

fn svg_of(k: &Kernel, args: Value) -> String {
    match k.invoke("typst.math-to-svg", Payload::Json(args)) {
        Ok(Payload::Binary(bytes)) => String::from_utf8(bytes.to_vec()).expect("utf8"),
        other => panic!("expected binary SVG, got {other:?}"),
    }
}

fn view_box(svg: &str) -> (f64, f64) {
    let i = svg.find("viewBox=\"0 0 ").expect("viewBox") + 13;
    let end = svg[i..].find('"').map(|j| i + j).expect("end");
    let mut it = svg[i..end]
        .split(' ')
        .map(|n| n.parse::<f64>().expect("number"));
    (it.next().expect("w"), it.next().expect("h"))
}

#[test]
fn a_formula_renders_to_outline_paths_in_the_requested_colour() {
    let k = kernel();
    let svg = svg_of(
        &k,
        json!({ "source": "integral_0^1 x^2 dif x = 1/3", "size": 24 }),
    );
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(!svg.contains("<text"), "glyphs are outlines, not text");
    assert!(!svg.contains("xlink"), "no namespace needed to nest it");
    assert!(svg.contains("currentColor") && !svg.contains("#010203"));
    let (w, h) = view_box(&svg);
    assert!(w > 40.0 && h > 15.0 && h < 80.0, "{w} x {h}");
    insta::assert_snapshot!("math_integral", svg);

    let blue = svg_of(
        &k,
        json!({ "source": "x^2", "size": 16, "color": "#0072b2" }),
    );
    assert!(blue.contains("#0072b2") && !blue.contains("currentColor"));
    // Deterministic.
    let again = svg_of(
        &k,
        json!({ "source": "integral_0^1 x^2 dif x = 1/3", "size": 24 }),
    );
    assert_eq!(svg, again);
}

#[test]
fn size_scales_the_box() {
    let k = kernel();
    let small = view_box(&svg_of(&k, json!({ "source": "a + b", "size": 12 })));
    let large = view_box(&svg_of(&k, json!({ "source": "a + b", "size": 24 })));
    assert!((large.0 / small.0 - 2.0).abs() < 0.1, "{small:?} {large:?}");
}

#[test]
fn bad_formulas_are_structured_errors_with_a_position() {
    let k = kernel();
    let err = k
        .invoke(
            "typst.math-to-svg",
            Payload::Json(json!({ "source": "a +\nb + nosuchfunction(3)" })),
        )
        .expect_err("unknown function");
    let text = err.to_string();
    let json = text.split_once("[math_error] ").expect("tagged").1;
    let end = json.rfind('}').expect("json end");
    let diag: Value = serde_json::from_str(&json[..=end]).expect("json");
    assert_eq!(diag["line"], 2, "{diag}");
    assert!(diag["column"].as_u64().expect("column") >= 3, "{diag}");
    for bad in [
        json!({ "source": "" }),
        json!({ "source": "x", "size": 0 }),
        json!({ "source": "x", "color": "red" }),
        json!({ "size": 12 }),
    ] {
        assert!(
            k.invoke("typst.math-to-svg", Payload::Json(bad)).is_err(),
            "must fail"
        );
    }
}
