//! Rendered figures next to their sources: `project.export-diagram`,
//! `project.cache-diagram`, `project.export-pdf` and `project.stat`, with the
//! real diagram and Typst plugins behind the kernel.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{EventBus, Kernel, Payload};
use serde_json::{Value, json};

const FLOW: &str = "flowchart TD\n  A[Start] --> B[Done]\n";
const BROKEN: &str = "flowchart TD\n  A[Start --> B\n";

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    root: PathBuf,
}

fn harness(engine: &str) -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("paper");
    let kernel = Kernel::new();
    let plugin = manang_project::ProjectPlugin::new(tmp.path().join("config"), EventBus::new())
        .with_debounce(Duration::from_millis(50));
    kernel
        .register(
            manang_project::manifest().expect("manifest"),
            Arc::new(plugin),
        )
        .expect("project");
    kernel
        .register(
            manang_diagram::manifest().expect("manifest"),
            Arc::new(manang_diagram::plugin()),
        )
        .expect("diagram");
    kernel
        .register(
            manang_typst::manifest().expect("manifest"),
            Arc::new(manang_typst::plugin(manang_typst::CompilerConfig::default()).expect("typst")),
        )
        .expect("typst");
    let h = Harness {
        _tmp: tmp,
        kernel,
        root,
    };
    h.ok(
        "project.create",
        json!({ "path": h.root.to_string_lossy(), "name": "Paper", "engine": engine }),
    );
    h
}

impl Harness {
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(other) => Err(format!("unexpected {other:?}")),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        self.call(command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    fn write(&self, rel: &str, text: &str) {
        self.ok(
            "project.write-file",
            json!({ "path": rel, "content": text }),
        );
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn export(&self, rel: &str) -> Value {
        self.ok("project.export-diagram", json!({ "path": rel }))
    }
}

fn modified(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .expect("mtime")
}

fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/Type/Page")
        .filter(|(at, _)| !text[at + "/Type/Page".len()..].starts_with('s'))
        .count()
}

#[test]
fn a_typst_project_gets_a_print_svg_and_no_pdf() {
    let h = harness("typst");
    h.write("figures/flow.mmd", FLOW);
    let out = h.export("figures/flow.mmd");
    assert_eq!(out["ok"], true);
    assert_eq!(out["svg"], "figures/flow.svg");
    assert_eq!(out["pdf"], Value::Null);
    assert_eq!(out["written"], json!(["figures/flow.svg"]));

    let svg = fs::read_to_string(h.path("figures/flow.svg")).expect("svg");
    let key = h.ok(
        "diagram.key",
        json!({ "language": "mermaid", "source": FLOW, "theme": "print" }),
    )["key"]
        .as_str()
        .expect("key")
        .to_owned();
    assert!(
        svg.starts_with(&format!("<!-- manang-diagram mermaid key={key} -->\n<svg")),
        "{}",
        &svg[..120.min(svg.len())]
    );
    assert!(!h.path("figures/flow.pdf").exists());
}

#[test]
fn mmd_to_svg_to_pdf_in_a_latex_project_and_unchanged_saves_write_nothing() {
    let h = harness("latex");
    h.write("figures/flow.mmd", FLOW);
    let out = h.export("figures/flow.mmd");
    assert_eq!(out["ok"], true);
    assert_eq!(out["pdf"], "figures/flow.pdf");
    assert_eq!(
        out["written"],
        json!(["figures/flow.svg", "figures/flow.pdf"])
    );

    let pdf = fs::read(h.path("figures/flow.pdf")).expect("pdf");
    assert!(pdf.starts_with(b"%PDF-"), "PDF magic bytes");
    assert_eq!(page_count(&pdf), 1, "one page");

    let svg_before = fs::read(h.path("figures/flow.svg")).expect("svg");
    let svg_time = modified(&h.path("figures/flow.svg"));
    let pdf_time = modified(&h.path("figures/flow.pdf"));
    std::thread::sleep(Duration::from_millis(20));

    // Saving again with the same source: nothing is rewritten.
    let again = h.export("figures/flow.mmd");
    assert_eq!(again["ok"], true);
    assert_eq!(again["written"], json!([]));
    assert_eq!(
        fs::read(h.path("figures/flow.svg")).expect("svg"),
        svg_before
    );
    assert_eq!(modified(&h.path("figures/flow.svg")), svg_time);
    assert_eq!(modified(&h.path("figures/flow.pdf")), pdf_time);

    // A deleted PDF comes back without touching the SVG.
    fs::remove_file(h.path("figures/flow.pdf")).expect("rm");
    let restored = h.export("figures/flow.mmd");
    assert_eq!(restored["written"], json!(["figures/flow.pdf"]));
    assert_eq!(modified(&h.path("figures/flow.svg")), svg_time);

    // A changed source rewrites both.
    h.write("figures/flow.mmd", "flowchart LR\n  A --> B --> C\n");
    let changed = h.export("figures/flow.mmd");
    assert_eq!(
        changed["written"],
        json!(["figures/flow.svg", "figures/flow.pdf"])
    );
    assert_ne!(
        fs::read(h.path("figures/flow.svg")).expect("svg"),
        svg_before
    );
}

#[test]
fn a_source_with_errors_keeps_the_previous_svg() {
    let h = harness("typst");
    h.write("figures/flow.mmd", FLOW);
    h.export("figures/flow.mmd");
    let good = fs::read(h.path("figures/flow.svg")).expect("svg");

    h.write("figures/flow.mmd", BROKEN);
    let out = h.export("figures/flow.mmd");
    assert_eq!(out["ok"], false);
    assert_eq!(
        out["svg"], "figures/flow.svg",
        "the old picture is still there"
    );
    assert_eq!(out["written"], json!([]));
    let error = out["error"].as_str().expect("error message");
    assert!(error.starts_with("[diagram_error] line 2:"), "{error}");
    assert_eq!(fs::read(h.path("figures/flow.svg")).expect("svg"), good);
}

#[test]
fn a_broken_new_diagram_writes_nothing() {
    let h = harness("typst");
    h.write("art/sketch.bob", "+--+\n|  |\n+--+\n");
    assert_eq!(h.export("art/sketch.bob")["ok"], true);
    h.write("figures/new.mmd", BROKEN);
    let out = h.export("figures/new.mmd");
    assert_eq!(out["ok"], false);
    assert_eq!(out["svg"], Value::Null);
    assert!(!h.path("figures/new.svg").exists());
}

#[test]
fn other_engines_and_tex_files() {
    let h = harness("markdown");
    h.write("figures/p.pikchr", "box \"A\"\narrow\nbox \"B\"\n");
    assert_eq!(h.export("figures/p.pikchr")["pdf"], Value::Null);
    // A .tex file anywhere makes the project want PDFs.
    h.write("appendix/table.tex", "\\begin{tabular}{l}x\\end{tabular}\n");
    let out = h.export("figures/p.pikchr");
    assert_eq!(out["pdf"], "figures/p.pdf");
    assert_eq!(out["written"], json!(["figures/p.pdf"]));
}

#[test]
fn bad_requests_are_errors() {
    let h = harness("typst");
    h.write("notes.txt", "x");
    assert!(
        h.call("project.export-diagram", json!({ "path": "notes.txt" }))
            .is_err()
    );
    assert!(
        h.call("project.export-diagram", json!({ "path": "nope.mmd" }))
            .is_err()
    );
    assert!(
        h.call("project.export-diagram", json!({ "path": "../x.mmd" }))
            .is_err()
    );
    assert!(h.call("project.export-diagram", json!({})).is_err());
    assert!(
        h.call(
            "project.cache-diagram",
            json!({ "language": "dot", "source": "x" })
        )
        .is_err()
    );
    assert!(
        h.call("project.export-pdf", json!({ "path": "notes.txt" }))
            .is_err()
    );
}

#[test]
fn inline_diagrams_go_to_an_ignored_cache() {
    let h = harness("markdown");
    // A project whose .gitignore does not mention .manang.
    fs::write(h.path(".gitignore"), "build/\n").expect("gitignore");
    let out = h.ok(
        "project.cache-diagram",
        json!({ "language": "mermaid", "source": FLOW }),
    );
    assert_eq!(out["ok"], true);
    let svg = out["svg"].as_str().expect("svg path");
    assert!(svg.starts_with(".manang/cache/diagrams/") && svg.ends_with(".svg"));
    assert!(h.path(svg).is_file());
    assert_eq!(out["written"], json!([svg]));
    let ignore = fs::read_to_string(h.path(".manang/cache/.gitignore")).expect("ignore");
    assert!(ignore.lines().any(|l| l == "*"));

    let again = h.ok(
        "project.cache-diagram",
        json!({ "language": "mermaid", "source": FLOW }),
    );
    assert_eq!(again["svg"], svg);
    assert_eq!(again["written"], json!([]));

    let broken = h.ok(
        "project.cache-diagram",
        json!({ "language": "mermaid", "source": BROKEN }),
    );
    assert_eq!(broken["ok"], false);
    assert!(broken["error"].as_str().is_some());
}

#[test]
fn figure_svgs_get_a_pdf_only_when_wanted() {
    let h = harness("typst");
    h.write("figures/sketch.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\"><rect width=\"40\" height=\"20\"/></svg>");
    let out = h.ok(
        "project.export-pdf",
        json!({ "path": "figures/sketch.svg" }),
    );
    assert_eq!(out["pdf"], Value::Null);
    assert!(!h.path("figures/sketch.pdf").exists());

    // Absolute paths inside the project work (the figure plugin uses them).
    let absolute = h.path("figures/sketch.svg");
    let forced = h.ok(
        "project.export-pdf",
        json!({ "path": absolute.to_string_lossy(), "force": true }),
    );
    assert_eq!(forced["pdf"], "figures/sketch.pdf");
    let pdf = fs::read(h.path("figures/sketch.pdf")).expect("pdf");
    assert!(pdf.starts_with(b"%PDF-"));

    let outside = tempfile::tempdir().expect("outside");
    let stray = outside.path().join("x.svg");
    fs::write(&stray, "<svg/>").expect("w");
    assert!(
        h.call(
            "project.export-pdf",
            json!({ "path": stray.to_string_lossy(), "force": true })
        )
        .is_err()
    );
}

#[test]
fn stat_reports_size_date_and_kind_without_reading() {
    let h = harness("typst");
    h.write("data/table.csv", "a,b\n1,2\n");
    let file = h.ok("project.stat", json!({ "path": "data/table.csv" }));
    assert_eq!(file["size"], 8);
    assert_eq!(file["kind"], "file");
    assert!(
        file["modified"]
            .as_u64()
            .is_some_and(|ms| ms > 1_600_000_000_000)
    );
    let dir = h.ok("project.stat", json!({ "path": "data" }));
    assert_eq!(dir["kind"], "dir");
    assert_eq!(dir["size"], 0);
    assert!(
        h.call("project.stat", json!({ "path": "missing.bin" }))
            .is_err()
    );
    assert!(
        h.call("project.stat", json!({ "path": "../outside" }))
            .is_err()
    );
}
