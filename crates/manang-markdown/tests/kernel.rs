//! Integration tests for the markdown plugin driven through the kernel.

use std::sync::Arc;

use manang_kernel::{Kernel, Payload};
use manang_markdown::{manifest, plugin};
use serde_json::{Value, json};

fn test_kernel() -> Kernel {
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

#[test]
fn manifest_is_native_and_commands_registered() {
    let m = manifest().expect("manifest");
    assert_eq!(m.plugin.name, "markdown");
    assert_eq!(m.plugin.tier, manang_kernel::Tier::Native);

    let k = test_kernel();
    let ids: Vec<_> = k.commands().into_iter().map(|c| c.id).collect();
    for id in [
        "markdown.render-html",
        "markdown.outline",
        "markdown.to-typst",
    ] {
        assert!(ids.contains(&id.to_owned()), "missing {id}");
    }
}

#[test]
fn render_html_with_frontmatter_and_math() {
    let source = r#"---
title: Scientific Markdown
author: Ada Lovelace
year: 2026
---

# Introduction to Energy

Energy equivalence is given by $E = mc^2$.

$$
\frac{\partial \rho}{\partial t} + \nabla \cdot \mathbf{j} = 0
$$

| Particle | Mass (MeV) |
| :--- | :--- |
| Electron | 0.511 |
| Proton | 938.272 |

- [x] Derivation verified
- [ ] Experimental confirmation
"#;

    let k = test_kernel();
    let res = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": source })),
        )
        .expect("render-html"),
    );

    // Frontmatter check
    let fm = &res["frontmatter"];
    assert_eq!(fm["format"], "yaml");
    assert_eq!(fm["data"]["title"], "Scientific Markdown");
    assert_eq!(fm["data"]["author"], "Ada Lovelace");
    assert_eq!(fm["data"]["year"], 2026);

    // HTML output check
    let html = res["html"].as_str().expect("html string");
    assert!(html.contains("<h1>Introduction to Energy</h1>"));
    assert!(html.contains("<span class=\"math math-inline\">$E = mc^2$</span>"));
    assert!(html.contains("<div class=\"math math-display\">"));
    assert!(html.contains("<table>"));
    assert!(html.contains("<input disabled=\"\" type=\"checkbox\" checked=\"\"/>"));

    // Headings check
    let headings = res["headings"].as_array().expect("headings array");
    assert_eq!(headings.len(), 1);
    assert_eq!(headings[0]["title"], "Introduction to Energy");
    assert_eq!(headings[0]["level"], 1);
}

#[test]
fn toml_frontmatter_parsing() {
    let source = r#"+++
title = "TOML Document"
draft = false
tags = ["rust", "wasm"]
+++

# Body
"#;

    let k = test_kernel();
    let res = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": source })),
        )
        .expect("render-html"),
    );

    let fm = &res["frontmatter"];
    assert_eq!(fm["format"], "toml");
    assert_eq!(fm["data"]["title"], "TOML Document");
    assert_eq!(fm["data"]["draft"], false);
    assert_eq!(fm["data"]["tags"][0], "rust");
}

#[test]
fn xss_mitigation_and_sanitization_vectors() {
    let k = test_kernel();

    // 1. Script tags must be escaped as text by default
    let xss1 = "<script>alert('pwned')</script>";
    let res1 = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": xss1 })),
        )
        .expect("render xss1"),
    );
    let html1 = res1["html"].as_str().expect("html");
    assert!(!html1.contains("<script>"), "raw script tag leaked!");
    assert!(html1.contains("&lt;script&gt;alert(&#39;pwned&#39;)&lt;/script&gt;"));

    // 2. Dangerous javascript: URLs must be sanitized to about:blank
    let xss2 = "[Click Me](javascript:alert(document.cookie))";
    let res2 = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": xss2 })),
        )
        .expect("render xss2"),
    );
    let html2 = res2["html"].as_str().expect("html");
    assert!(!html2.contains("javascript:"), "javascript url leaked!");
    assert!(html2.contains("<a href=\"about:blank\">Click Me</a>"));

    // 3. Obfuscated whitespace / mixed-case javascript: URLs
    let xss3 = "[Tricky]( \r\n\t JaVaScRiPt:alert(1) )";
    let res3 = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": xss3 })),
        )
        .expect("render xss3"),
    );
    let html3 = res3["html"].as_str().expect("html");
    assert!(
        !html3.contains("javascript:"),
        "mixed-case javascript url leaked!"
    );
    assert!(html3.contains("<a href=\"about:blank\">Tricky</a>"));

    // 4. Data URI HTML injection
    let xss4 = "[Data](data:text/html,<script>alert(1)</script>)";
    let res4 = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": xss4 })),
        )
        .expect("render xss4"),
    );
    let html4 = res4["html"].as_str().expect("html");
    assert!(!html4.contains("data:text/html"));
    assert!(html4.contains("<a href=\"about:blank\">Data</a>"));

    // 5. Onerror attribute injection in raw HTML
    let xss5 = "<img src=x onerror=alert(1)>";
    let res5 = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": xss5 })),
        )
        .expect("render xss5"),
    );
    let html5 = res5["html"].as_str().expect("html");
    assert!(
        !html5.contains("<img"),
        "img tag leaked when raw HTML disabled!"
    );
    assert!(html5.contains("&lt;img src=x onerror=alert(1)&gt;"));

    // 6. Allowed raw HTML with allowlist permits safe tags but blocks unsafe ones
    let mixed = "<b>bold text</b> <script>bad()</script> <i onclick=foo()>italic</i>";
    let res6 = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({
                "source": mixed,
                "allow_raw_html": true,
                "allowed_html_tags": ["b", "i"]
            })),
        )
        .expect("render mixed"),
    );
    let html6 = res6["html"].as_str().expect("html");
    assert!(
        html6.contains("<b>bold text</b>"),
        "safe allowed tag <b> missing"
    );
    assert!(!html6.contains("<script>"), "disallowed <script> leaked");
    assert!(html6.contains("&lt;script&gt;bad()&lt;/script&gt;"));
    assert!(!html6.contains("<i onclick"), "tag with onclick leaked");
    assert!(html6.contains("&lt;i onclick=foo()&gt;"));
}

#[test]
fn outline_extraction_with_positions() {
    let source = "# Main Title\n\nSome text\n\n## Sub Section {#sub}\n\n### Deep Sub\n";
    let k = test_kernel();
    let res = json_of(
        k.invoke(
            "markdown.outline",
            Payload::Json(json!({ "source": source })),
        )
        .expect("outline"),
    );

    let headings = res["headings"].as_array().expect("array");
    assert_eq!(headings.len(), 3);

    assert_eq!(headings[0]["title"], "Main Title");
    assert_eq!(headings[0]["level"], 1);
    assert_eq!(headings[0]["line"], 1);

    assert_eq!(headings[1]["title"], "Sub Section");
    assert_eq!(headings[1]["level"], 2);
    assert_eq!(headings[1]["id"], "sub");
    assert_eq!(headings[1]["line"], 5);

    assert_eq!(headings[2]["title"], "Deep Sub");
    assert_eq!(headings[2]["level"], 3);
    assert_eq!(headings[2]["line"], 7);
}

#[test]
fn markdown_to_typst_conversion_and_diagnostics() {
    let source = r#"# Document Title

This is **bold** and *italic* and `code` and ~~deleted~~.

> Quoted wisdom from Landauer.

1. First point
2. Second point

- Unordered item
- Another item

$E = mc^2$

$$
\int_0^1 x^2 dx = \frac{1}{3}
$$

[Manang Project](https://github.com/Fundaments-Work/Manang)

![Figure](figures/diagram.svg)

| Metric | Target |
| :--- | :--- |
| Latency | < 8ms |

<div class="custom">Raw HTML not supported</div>
"#;

    let k = test_kernel();
    let res = json_of(
        k.invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": source })),
        )
        .expect("to-typst"),
    );

    let typst = res["typst"].as_str().expect("typst string");
    let diags = res["diagnostics"].as_array().expect("diagnostics array");

    assert!(typst.contains("= Document Title"));
    assert!(typst.contains("*bold*"));
    assert!(typst.contains("_italic_"));
    assert!(typst.contains("`code`"));
    assert!(typst.contains("#strike[deleted]"));
    assert!(typst.contains("#quote(block: true)["));
    assert!(typst.contains("+ First point"));
    assert!(typst.contains("- Unordered item"));
    assert!(typst.contains("$E = mc^2$"));
    assert!(typst.contains("$ \\int_0^1 x^2 dx = \\frac{1}{3} $"));
    assert!(typst.contains("#link(\"https://github.com/Fundaments-Work/Manang\")[Manang Project]"));
    assert!(typst.contains("#image(\"figures/diagram.svg\")"));
    assert!(typst.contains("#table("));

    // Unsupported raw HTML must be reported in diagnostics
    assert!(
        diags
            .iter()
            .any(|d| d["message"].as_str().unwrap().contains("raw HTML"))
    );
    assert!(!typst.contains("<div"));
}

#[test]
fn insta_snapshots_html_and_typst() {
    let sample = r#"# Quantum Formalism

A state vector $|psi rangle$ satisfies:

$$
i hbar \frac{d}{dt} |\psi\rangle = \hat{H} |\psi\rangle
$$

| Observable | Operator |
| :--- | :--- |
| Momentum | $-i \hbar \nabla$ |
"#;

    let k = test_kernel();
    let html_res = json_of(
        k.invoke(
            "markdown.render-html",
            Payload::Json(json!({ "source": sample })),
        )
        .expect("html"),
    );
    insta::assert_snapshot!("markdown_rendered_html", html_res["html"].as_str().unwrap());

    let typst_res = json_of(
        k.invoke(
            "markdown.to-typst",
            Payload::Json(json!({ "source": sample })),
        )
        .expect("typst"),
    );
    insta::assert_snapshot!("markdown_to_typst", typst_res["typst"].as_str().unwrap());
}
