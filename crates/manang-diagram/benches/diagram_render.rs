//! Render latency of the diagram engines.
//!
//! Budget: a 50-node flowchart renders in under 20 ms on the optimised dev
//! profile (`cargo bench -p manang-diagram`).

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_diagram::{Theme, plugin};

/// A layered flowchart: 50 nodes, each wired to the next and some to a
/// node two steps ahead, so layout has crossings to resolve.
fn flowchart(nodes: usize) -> String {
    let mut src = String::from("flowchart TD\n");
    for i in 0..nodes {
        let _ = writeln!(src, "  n{i}[Node {i}]");
    }
    for i in 0..nodes.saturating_sub(1) {
        let _ = writeln!(src, "  n{i} --> n{}", i + 1);
        if i % 3 == 0 && i + 2 < nodes {
            let _ = writeln!(src, "  n{i} -.-> n{}", i + 2);
        }
    }
    src
}

fn bench(c: &mut Criterion) {
    let diagrams = plugin();
    let source = flowchart(50);
    // Build the per-theme renderer outside the measurement.
    let _ = diagrams.render("mermaid", &source, Theme::Dark);
    c.bench_function("mermaid_flowchart_50_nodes", |b| {
        b.iter(|| diagrams.render("mermaid", black_box(&source), Theme::Dark))
    });
    c.bench_function("mermaid_check_flowchart_50_nodes", |b| {
        b.iter(|| diagrams.check("mermaid", black_box(&source)))
    });
    let pikchr = "box \"A\"; arrow; box \"B\"; arrow; circle \"C\"; arrow; box \"D\"";
    c.bench_function("pikchr_small", |b| {
        b.iter(|| diagrams.render("pikchr", black_box(pikchr), Theme::Dark))
    });
    let bob = "+------+   +------+\n| one  |-->| two  |\n+------+   +------+\n";
    c.bench_function("svgbob_small", |b| {
        b.iter(|| diagrams.render("svgbob", black_box(bob), Theme::Dark))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
