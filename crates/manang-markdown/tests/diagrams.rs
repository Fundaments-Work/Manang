//! Diagram fences: placeholders in the preview HTML, `markdown.diagrams`,
//! and figures in `markdown.to-typst`, through the kernel with the real
//! project and diagram plugins.

use std::sync::Arc;

use manang_kernel::{EventBus, Kernel, Payload};
use manang_markdown::{FenceFigure, fence_language, markdown_to_typst_with};
use serde_json::{Value, json};

const DOC: &str = r#"---
title: Fences
---

# Pictures

```mermaid
flowchart TD
  A[Start] --> B[Done]
```

Some *text* between.

~~~bob
+--+
|  |
+--+
~~~

```rust
fn main() {}
```

<figure class="mng-diagram" data-diagram="0000000000000000"></figure>
"#;

const BROKEN: &str = "# Broken\n\n```mermaid\nflowchart TD\n  A[Start --> B\n```\n";

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn markdown_only() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manang_markdown::manifest().expect("manifest"),
            Arc::new(manang_markdown::plugin()),
        )
        .expect("markdown");
    kernel
}

/// Markdown, diagram and project plugins, with a Markdown project open.
fn full(dir: &std::path::Path) -> Kernel {
    let kernel = markdown_only();
    kernel
        .register(
            manang_diagram::manifest().expect("manifest"),
            Arc::new(manang_diagram::plugin()),
        )
        .expect("diagram");
    kernel
        .register(
            manang_project::manifest().expect("manifest"),
            Arc::new(manang_project::ProjectPlugin::new(
                dir.join("config"),
                EventBus::new(),
            )),
        )
        .expect("project");
    kernel
        .invoke(
            "project.create",
            Payload::Json(json!({
                "path": dir.join("notes").to_string_lossy(),
                "name": "Notes",
                "engine": "markdown",
            })),
        )
        .expect("project");
    kernel
}

#[test]
fn the_preview_gets_placeholders_and_the_fence_list() {
    let k = markdown_only();
    let out = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": DOC })),
        )
        .expect("render"),
    );
    let html = out["html"].as_str().expect("html");
    insta::assert_snapshot!("diagram_placeholders_html", html);
    // A placeholder typed into the document is raw HTML: escaped.
    assert!(html.contains("&lt;figure class=&quot;mng-diagram&quot;"));
    assert_eq!(html.matches("<figure class=\"mng-diagram\"").count(), 2);

    let diagrams = &out["diagrams"];
    insta::assert_json_snapshot!("diagram_fences", diagrams);
    for fence in diagrams.as_array().expect("list") {
        let key = fence["key"].as_str().expect("key");
        let line = fence["line"].as_u64().expect("line");
        assert!(
            html.contains(&format!("data-diagram=\"{key}\""))
                && html.contains(&format!("data-line=\"{line}\"")),
            "placeholder for {fence}"
        );
    }

    let listed = json_of(
        k.invoke("markdown.diagrams", Payload::Json(json!({ "source": DOC })))
            .expect("diagrams"),
    );
    assert_eq!(&listed["diagrams"], diagrams);
    assert!(
        k.invoke("markdown.diagrams", Payload::Json(json!({})))
            .is_err()
    );
}

#[test]
fn frontmatter_does_not_shift_diagnostic_lines() {
    let k = markdown_only();
    let source = "---\na: 1\n---\n\n<script>x</script>\n";
    let out = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": source })),
        )
        .expect("render"),
    );
    let lines: Vec<_> = out["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|d| d["line"].as_u64())
        .collect();
    assert!(lines.contains(&5), "{lines:?}");
}

#[test]
fn to_typst_figures_come_from_the_fence_hook() {
    let mut calls = Vec::new();
    let (typst, diagnostics) = markdown_to_typst_with(DOC, 0, &mut |fence| {
        calls.push((fence.language.clone(), fence.line));
        Some(match fence.language.as_str() {
            "mermaid" => FenceFigure::Image(format!("/.manang/cache/diagrams/{}.svg", fence.key)),
            _ => FenceFigure::Keep("not today".to_owned()),
        })
    });
    assert_eq!(
        calls,
        vec![("mermaid".to_owned(), 7), ("svgbob".to_owned(), 14)]
    );
    insta::assert_snapshot!("diagram_fences_to_typst", typst);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message == "not today" && d.line == Some(14))
    );
}

#[test]
fn to_typst_renders_fences_into_the_project_cache() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = full(dir.path());
    let out = json_of(
        k.invoke("markdown.to-typst", Payload::Json(json!({ "source": DOC })))
            .expect("to-typst"),
    );
    let typst = out["typst"].as_str().expect("typst");
    let images: Vec<&str> = typst
        .lines()
        .filter(|l| l.starts_with("#figure(image(\"/.manang/cache/diagrams/"))
        .collect();
    assert_eq!(images.len(), 2, "{typst}");
    for line in images {
        let path = line
            .trim_start_matches("#figure(image(\"/")
            .trim_end_matches("\"))");
        let svg = std::fs::read_to_string(dir.path().join("notes").join(path)).expect("svg");
        assert!(svg.starts_with("<!-- manang-diagram "), "{path}");
    }
    assert!(!typst.contains("```mermaid"));
    assert!(typst.contains("```rust"), "ordinary code stays code");
}

#[test]
fn a_broken_fence_stays_code_with_a_warning_at_its_line() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = full(dir.path());
    let out = json_of(
        k.invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": BROKEN })),
        )
        .expect("to-typst"),
    );
    assert!(out["typst"].as_str().expect("typst").contains("```mermaid"));
    let warning = out["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|d| d["severity"] == "warning")
        .expect("a warning");
    assert_eq!(warning["line"], 3, "the fence's line");
    let message = warning["message"].as_str().expect("message");
    assert!(message.contains("line 5:"), "document line: {message}");
}

#[test]
fn without_a_project_fences_stay_code() {
    let k = markdown_only();
    let out = json_of(
        k.invoke("markdown.to-typst", Payload::Json(json!({ "source": DOC })))
            .expect("to-typst"),
    );
    assert!(out["typst"].as_str().expect("typst").contains("```mermaid"));
    let warnings = out["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|d| {
            d["severity"] == "warning"
                && d["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("stays a code block"))
        })
        .count();
    assert_eq!(warnings, 2);
}

#[test]
fn fence_names_match_the_diagram_plugin() {
    let kernel = Kernel::new();
    kernel
        .register(
            manang_diagram::manifest().expect("manifest"),
            Arc::new(manang_diagram::plugin()),
        )
        .expect("diagram");
    let languages = json_of(
        kernel
            .invoke("diagram.languages", Payload::Empty)
            .expect("languages"),
    );
    let mut seen = 0;
    for language in languages["languages"].as_array().expect("list") {
        let id = language["id"].as_str().expect("id");
        for fence in language["fences"].as_array().expect("fences") {
            let fence = fence.as_str().expect("fence");
            assert_eq!(fence_language(fence), Some(id), "fence {fence}");
            seen += 1;
        }
    }
    assert_eq!(seen, 4);
}
