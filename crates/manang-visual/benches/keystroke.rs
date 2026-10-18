//! The keystroke path of the Visual editor on a 1 MB document: one edit, the
//! incremental reparse, and the encoded span table of a viewport slice, as
//! `visual.edit` followed by `visual.blocks` would do. Budget (AGENTS.md):
//! keystroke p99 under 8 ms.
//!
//! The percentile report runs first and fails the bench when the budget is
//! broken; criterion then times the individual steps.

use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{Criterion, black_box};
use manang_markdown::visual::MarkdownAdapter;
use manang_typst::visual::TypstAdapter;
use manang_visual::{Change, SessionHost};

const TARGET_BYTES: usize = 1024 * 1024;
const VIEWPORT: usize = 40_000;
const BUDGET: Duration = Duration::from_millis(8);

fn markdown_doc() -> String {
    let unit = "# Section heading\n\nA paragraph with **bold**, *emphasis*, `code`, a [link](https://example.org/x) and inline math $a^2 + b^2 = c^2$ that goes on for a while so lines are realistic.\n\n- first item with *style*\n- second item\n  - nested item\n\n1. ordered one\n2. ordered two\n\n> a quoted line with **strong** text\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n![figure](figs/a.png)\n\n";
    unit.repeat(TARGET_BYTES / unit.len() + 1)
}

fn typst_doc() -> String {
    let unit = "= Section heading <sec>\n\nA paragraph with *bold*, _emphasis_, `code`, see @sec and inline math $a^2 + b^2 = c^2$ that goes on for a while so lines are realistic. Visit #link(\"https://example.org\")[the site].\n\n- first item with _style_\n- second item\n  - nested item\n\n+ ordered one\n+ ordered two\n\n#set text(size: 11pt)\n\n$ integral_0^1 x^2 dif x $\n\n#figure(\n  image(\"figs/a.svg\", width: 80%),\n  caption: [A figure],\n)\n\n```rust\nfn main() {}\n```\n\n";
    unit.repeat(TARGET_BYTES / unit.len() + 1)
}

fn host_with(adapter: &str, doc: &str) -> (SessionHost, u32) {
    let host = SessionHost::new();
    host.register(Arc::new(MarkdownAdapter::new()))
        .expect("register markdown");
    host.register(Arc::new(TypstAdapter::new()))
        .expect("register typst");
    let open = host.open(doc, Some(adapter), None).expect("open");
    (host, open.session)
}

/// One keystroke: insert a character, then fetch the table of the viewport
/// around the caret, encoded as the IPC response.
fn keystroke(host: &SessionHost, id: u32, at: usize, len: usize) -> usize {
    keystroke_timed(host, id, at, len).0
}

/// [`keystroke`], also returning how long the edit alone took.
fn keystroke_timed(host: &SessionHost, id: u32, at: usize, len: usize) -> (usize, Duration) {
    let t = Instant::now();
    let out = host
        .with(id, |s| {
            s.edit(
                &[Change {
                    from: at,
                    to: at,
                    insert: "x".to_owned(),
                }],
                len + 1,
            )
        })
        .expect("edit");
    let edit_time = t.elapsed();
    let from = at.saturating_sub(VIEWPORT / 2);
    let table = host
        .with(id, |s| s.blocks(from, from + VIEWPORT))
        .expect("blocks");
    black_box(&out);
    (table.encode().len(), edit_time)
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let ix = ((sorted.len() as f64 * p).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[ix]
}

fn report(name: &str, adapter: &str, doc: &str) -> Duration {
    let (host, id) = host_with(adapter, doc);
    let doc_len = doc.len(); // the documents are ASCII: bytes == UTF-16 units
    let start_at = doc_len / 2;
    let mut samples = Vec::with_capacity(3000);
    let mut edits = Vec::with_capacity(3000);
    let mut bytes = 0;
    for i in 0..3000 {
        let at = start_at + i;
        let t = Instant::now();
        let (b, edit) = keystroke_timed(&host, id, at, doc_len + i);
        bytes = b;
        samples.push(t.elapsed());
        edits.push(edit);
    }
    edits.sort();
    println!(
        "  edit alone {name:>9}: p50 {:>8.3?}  p99 {:>8.3?}",
        percentile(&edits, 0.50),
        percentile(&edits, 0.99)
    );
    samples.sort();
    let p99 = percentile(&samples, 0.99);
    println!(
        "keystroke {name:>9} 1 MB: p50 {:>8.3?}  p95 {:>8.3?}  p99 {:>8.3?}  max {:>8.3?}  (table {bytes} B)",
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        p99,
        samples[samples.len() - 1],
    );
    p99
}

fn criterion_benches(c: &mut Criterion) {
    for (name, adapter, doc) in [
        ("markdown", "markdown", markdown_doc()),
        ("typst", "typst", typst_doc()),
    ] {
        let mut g = c.benchmark_group(format!("visual_{name}_1mb"));
        g.sample_size(30);
        g.bench_function("open (full parse)", |b| {
            b.iter(|| {
                let host = SessionHost::new();
                host.register(Arc::new(MarkdownAdapter::new()))
                    .expect("register");
                host.register(Arc::new(TypstAdapter::new()))
                    .expect("register");
                black_box(host.open(&doc, Some(adapter), None).expect("open"));
            });
        });
        let (host, id) = host_with(adapter, &doc);
        let mid = doc.len() / 2;
        g.bench_function("blocks (40 KB viewport, encoded)", |b| {
            b.iter(|| {
                let t = host
                    .with(id, |s| s.blocks(mid, mid + VIEWPORT))
                    .expect("blocks");
                black_box(t.encode());
            });
        });
        let mut len = doc.len();
        let mut at = mid;
        g.bench_function("keystroke (edit + blocks + encode)", |b| {
            b.iter(|| {
                black_box(keystroke(&host, id, at, len));
                at += 1;
                len += 1;
            });
        });
        g.bench_function("run_command bold", |b| {
            b.iter(|| {
                black_box(
                    host.with(id, |s| s.run_command("bold", mid, mid + 5, ""))
                        .expect("plan"),
                );
            });
        });
        g.finish();
    }
}

fn main() {
    let mut worst = Duration::ZERO;
    worst = worst.max(report("markdown", "markdown", &markdown_doc()));
    worst = worst.max(report("typst", "typst", &typst_doc()));
    assert!(
        worst < BUDGET,
        "keystroke p99 {worst:?} exceeds the {BUDGET:?} budget"
    );
    let mut c = Criterion::default().configure_from_args();
    criterion_benches(&mut c);
    c.final_summary();
}
