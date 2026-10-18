//! ADR 0012 performance targets: one operation on a 2,000-shape scene in
//! well under a millisecond, 1,000 operations with undo history, and the
//! SVG export of 2,000 shapes.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_figure::{
    Binding, ConnectorNode, FigureDocument, FigureNode, FigureOp, FigureScene, ShapeNode, Style,
    SvgExportOptions, Vec2, render_svg,
};

/// A grid of `n` labelled shapes, every second one linked to the next by a
/// bound arrow (so moves re-route).
fn scene(n: usize, roughness: u8) -> FigureScene {
    let mut doc = FigureDocument::new(1, 4000.0, 4000.0);
    let mut nodes = Vec::with_capacity(n);
    let shapes = n * 2 / 3;
    for i in 0..shapes {
        let (col, row) = ((i % 40) as f64, (i / 40) as f64);
        let mut s = ShapeNode::new(format!("s{i}"), col * 160.0, row * 100.0, 120.0, 60.0);
        s.label = Some(format!("Node {i}"));
        s.style = Style {
            roughness: Some(roughness),
            ..Style::default()
        };
        nodes.push(if i % 3 == 0 {
            FigureNode::Ellipse(s)
        } else {
            FigureNode::Rect(s)
        });
    }
    for i in 0..(n - shapes) {
        let mut c = ConnectorNode::new(format!("c{i}"), Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        c.start = Some(Binding::to(format!("s{i}")));
        c.end = Some(Binding::to(format!("s{}", (i + 1) % shapes)));
        nodes.push(FigureNode::Arrow(c));
    }
    if doc.apply(&FigureOp::Add { nodes, index: None }).is_err() {
        return doc.scene;
    }
    doc.scene
}

fn bench(c: &mut Criterion) {
    let big = scene(2000, 0);

    c.bench_function("apply one move on 2,000 shapes", |b| {
        let mut doc = FigureDocument::from_scene(1, None, big.clone());
        b.iter(|| {
            let _ = black_box(doc.apply(&FigureOp::Move {
                ids: vec!["s10".to_owned()],
                dx: 1.0,
                dy: 0.0,
            }));
        });
    });

    c.bench_function("apply 1,000 ops with history", |b| {
        b.iter(|| {
            let mut doc = FigureDocument::from_scene(1, None, big.clone());
            for i in 0..1000 {
                let id = format!("s{}", i % 1000);
                let op = match i % 4 {
                    0 => FigureOp::Move {
                        ids: vec![id],
                        dx: 2.0,
                        dy: 1.0,
                    },
                    1 => FigureOp::SetStyle {
                        ids: vec![id],
                        style: Some(Style {
                            stroke: Some("blue".to_owned()),
                            ..Style::default()
                        }),
                        id: None,
                        stroke: None,
                        fill: None,
                        size: None,
                        color: None,
                    },
                    2 => FigureOp::Undo,
                    _ => FigureOp::Redo,
                };
                let _ = black_box(doc.apply(&op));
            }
        });
    });

    c.bench_function("render 2,000 shapes to SVG (clean)", |b| {
        b.iter(|| black_box(render_svg(&big, &SvgExportOptions::default())));
    });

    let rough = scene(2000, 1);
    c.bench_function("render 2,000 shapes to SVG (hand-drawn)", |b| {
        b.iter(|| black_box(render_svg(&rough, &SvgExportOptions::default())));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
