//! Integration tests for the diagram plugin driven through the kernel.

use std::sync::Arc;

use manang_diagram::{manifest, plugin};
use manang_kernel::{Kernel, KernelError, Payload, PluginError, Tier};
use serde_json::{Value, json};

fn kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest is valid"), Arc::new(plugin()))
        .expect("registers");
    kernel
}

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn failure(result: Result<Payload, KernelError>) -> String {
    match result {
        Err(KernelError::Plugin(PluginError::Failed(message))) => message,
        other => panic!("expected a plugin failure, got {other:?}"),
    }
}

#[test]
fn manifest_is_native_pure_and_declares_every_command() {
    let m = manifest().expect("manifest");
    assert_eq!(m.plugin.name, "diagram");
    assert_eq!(m.plugin.tier, Tier::Native);
    let ids: Vec<_> = kernel().commands().into_iter().map(|c| c.id).collect();
    for id in [
        "diagram.languages",
        "diagram.check",
        "diagram.render",
        "diagram.key",
    ] {
        assert!(ids.contains(&id.to_owned()), "missing {id}");
    }
}

#[test]
fn languages_match_the_frontend_contract() {
    let out = json_of(
        kernel()
            .invoke("diagram.languages", Payload::Json(json!({})))
            .expect("languages"),
    );
    assert_eq!(
        out,
        json!({ "languages": [
            { "id": "mermaid", "name": "Mermaid", "extensions": [".mmd", ".mermaid"],
              "fences": ["mermaid"], "engine": "merman 0.9.0" },
            { "id": "pikchr", "name": "Pikchr", "extensions": [".pikchr"],
              "fences": ["pikchr"], "engine": "pikru 1.0.1" },
            { "id": "svgbob", "name": "ASCII diagram", "extensions": [".bob"],
              "fences": ["bob", "svgbob"], "engine": "svgbob 0.7.2" },
        ]})
    );
}

#[test]
fn render_returns_svg_bytes_for_every_language_and_theme() {
    let k = kernel();
    for (language, source) in [
        ("mermaid", "flowchart LR\n  A --> B\n"),
        ("pikchr", "box \"A\"; arrow; box \"B\""),
        ("svgbob", "+--+\n|  |\n+--+"),
    ] {
        for theme in ["dark", "light", "print"] {
            let out = k
                .invoke(
                    "diagram.render",
                    Payload::Json(
                        json!({ "language": language, "source": source, "theme": theme }),
                    ),
                )
                .unwrap_or_else(|e| panic!("{language}/{theme}: {e}"));
            let Payload::Binary(bytes) = out else {
                panic!("{language}: expected binary SVG");
            };
            let svg = std::str::from_utf8(&bytes).expect("UTF-8");
            assert!(svg.starts_with("<svg"), "{language}/{theme}: {svg:.40}");
        }
    }
}

#[test]
fn render_errors_carry_the_contract_prefixes() {
    let k = kernel();
    let message = failure(k.invoke(
        "diagram.render",
        Payload::Json(json!({
            "language": "mermaid",
            "source": "flowchart TD\n  A[ok] --> B(\n",
            "theme": "dark",
        })),
    ));
    assert!(message.starts_with("[diagram_error] line 2:"), "{message}");

    let message = failure(k.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": "graphviz", "source": "digraph {}", "theme": "dark" })),
    ));
    assert!(message.starts_with("[unknown_language]"), "{message}");

    let message = failure(k.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": "svgbob", "source": "", "theme": "sepia" })),
    ));
    assert!(message.starts_with("[invalid_argument]"), "{message}");
}

#[test]
fn check_reports_positions_in_characters() {
    let out = json_of(
        kernel()
            .invoke(
                "diagram.check",
                Payload::Json(json!({
                    "language": "mermaid",
                    // "ü" and "→" are multi-byte; the error is at the `(`.
                    "source": "flowchart TD\n  ü[→] --> B(\n",
                })),
            )
            .expect("check"),
    );
    let d = &out["diagnostics"][0];
    assert_eq!(d["severity"], "error");
    assert_eq!(d["line"], 2);
    assert_eq!(d["column"], 13, "{d}");
    assert!(d["endColumn"].as_u64() > d["column"].as_u64());
    assert!(d["message"].as_str().is_some_and(|m| !m.is_empty()));

    let clean = json_of(
        kernel()
            .invoke(
                "diagram.check",
                Payload::Json(json!({ "language": "svgbob", "source": "anything (" })),
            )
            .expect("check"),
    );
    assert_eq!(clean, json!({ "diagnostics": [] }));
}

#[test]
fn key_is_stable_and_depends_on_every_input() {
    let k = kernel();
    let key = |language: &str, source: &str, theme: &str| {
        json_of(
            k.invoke(
                "diagram.key",
                Payload::Json(json!({ "language": language, "source": source, "theme": theme })),
            )
            .expect("key"),
        )["key"]
            .as_str()
            .expect("hex key")
            .to_owned()
    };
    let base = key("mermaid", "graph TD\nA-->B", "dark");
    assert_eq!(base.len(), 64);
    assert!(base.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(base, key("mermaid", "graph TD\nA-->B", "dark"));
    assert_ne!(base, key("mermaid", "graph TD\nA-->C", "dark"));
    assert_ne!(base, key("mermaid", "graph TD\nA-->B", "print"));
    assert_ne!(key("pikchr", "box", "dark"), key("svgbob", "box", "dark"));
}
