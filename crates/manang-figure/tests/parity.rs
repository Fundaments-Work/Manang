//! The fixture that keeps the TypeScript mirror (`src/lib/figure/*.ts`,
//! the browser mock) in step with this crate: operation sequences and the
//! scenes they produce, outline paths in every style, text wrapping and
//! routing. `UPDATE_PARITY=1 cargo test -p manang-figure --test parity`
//! rewrites it; otherwise the test fails when Rust output drifts from it,
//! and `src/lib/figure/parity.test.ts` fails when TypeScript does.

use manang_figure::sketch::{self, Rng};
use manang_figure::text::wrap;
use manang_figure::{FigureDocument, FigureNode, FigureOp, FigureScene, FontFamily, Rect, route};
use serde_json::{Value, json};

fn shape(kind: &str, id: &str, x: f64, y: f64, extra: Value) -> Value {
    let mut v = json!({ "kind": kind, "id": id, "x": x, "y": y, "width": 120, "height": 60 });
    if let (Value::Object(map), Value::Object(more)) = (&mut v, extra) {
        map.extend(more);
    }
    v
}

fn op_cases() -> Vec<(&'static str, Vec<Value>)> {
    let a = shape("rect", "a", 0.0, 0.0, json!({ "label": "A" }));
    let b = shape("diamond", "b", 300.0, 0.0, json!({ "label": "B" }));
    let c = shape("ellipse", "c", 0.0, 200.0, json!({}));
    let arrow = json!({ "kind": "arrow", "id": "e1", "points": [{ "x": 0, "y": 0 }, { "x": 1, "y": 1 }],
        "start": { "node": "a" }, "end": { "node": "b" } });
    let elbow = json!({ "kind": "arrow", "id": "e2", "points": [{ "x": 0, "y": 0 }, { "x": 1, "y": 1 }],
        "start": { "node": "a" }, "end": { "node": "c", "anchor": { "x": 1, "y": 0.5 } }, "routing": "elbow" });
    let base = json!({ "op": "add", "nodes": [a, b, c, arrow, elbow] });
    vec![
        ("add and route", vec![base.clone()]),
        (
            "move drags bound arrows",
            vec![
                base.clone(),
                json!({ "op": "move", "ids": ["b"], "dx": 40, "dy": 150 }),
            ],
        ),
        (
            "moving an arrow alone unbinds it",
            vec![
                base.clone(),
                json!({ "op": "move", "ids": ["e1"], "dx": 5, "dy": 5 }),
            ],
        ),
        (
            "update merges fields and style",
            vec![
                base.clone(),
                json!({ "op": "update", "id": "a", "set": { "label": "Start", "kind": "ellipse", "style": { "fill": "blue" } } }),
                json!({ "op": "update", "id": "a", "set": { "label": null, "style": { "fill": null, "roughness": 1 } } }),
            ],
        ),
        (
            "resize and rotate",
            vec![
                base.clone(),
                json!({ "op": "resize", "id": "c", "x": 10, "y": 220, "width": -80, "height": 40 }),
                json!({ "op": "rotate", "ids": ["b"], "angle": -90 }),
            ],
        ),
        (
            "remove unbinds",
            vec![base.clone(), json!({ "op": "remove", "ids": ["b"] })],
        ),
        (
            "reorder, group, ungroup",
            vec![
                base.clone(),
                json!({ "op": "reorder", "ids": ["a"], "to": "front" }),
                json!({ "op": "reorder", "ids": ["c"], "to": "backward" }),
                json!({ "op": "group", "ids": ["a", "b"] }),
                json!({ "op": "group", "ids": ["a", "c"], "group": "outer" }),
                json!({ "op": "ungroup", "ids": ["a"] }),
            ],
        ),
        (
            "bind, align, distribute",
            vec![
                base.clone(),
                json!({ "op": "bind", "id": "e1", "end": "end", "node": "c", "anchor": { "x": 0.5, "y": 0 } }),
                json!({ "op": "align", "ids": ["a", "b", "c"], "edge": "right" }),
                json!({ "op": "distribute", "ids": ["a", "b", "c"], "axis": "vertical" }),
                json!({ "op": "bind", "id": "e2", "end": "start", "node": null }),
            ],
        ),
        (
            "style, canvas, batch, clear",
            vec![
                base.clone(),
                json!({ "op": "set-style", "ids": ["a", "e1"], "style": { "stroke": "vermillion", "strokeStyle": "dashed" } }),
                json!({ "op": "set-canvas", "width": 400, "height": 300 }),
                json!({ "op": "batch", "ops": [{ "op": "move", "ids": ["a"], "dx": 1, "dy": 2 }, { "op": "remove", "ids": ["c"] }] }),
                json!({ "op": "clear" }),
            ],
        ),
        (
            "failures change nothing",
            vec![
                base.clone(),
                json!({ "op": "add", "nodes": [shape("rect", "a", 0.0, 0.0, json!({}))] }),
                json!({ "op": "move", "ids": ["ghost"], "dx": 1, "dy": 1 }),
                json!({ "op": "update", "id": "a", "set": { "style": { "stroke": "red" } } }),
                json!({ "op": "batch", "ops": [{ "op": "move", "ids": ["a"], "dx": 1, "dy": 1 }, { "op": "remove", "ids": ["ghost"] }] }),
                json!({ "op": "group", "ids": ["a"] }),
                json!({ "op": "bind", "id": "a", "end": "start", "node": "b" }),
            ],
        ),
        (
            "v1 operations",
            vec![
                json!({ "op": "add-node", "node": shape("rect", "n1", 5.0, 5.0, json!({})) }),
                json!({ "op": "translate", "id": "n1", "dx": 10, "dy": 0 }),
                json!({ "op": "set-style", "id": "n1", "stroke": "#ffffff", "fill": "surface" }),
                json!({ "op": "update-node", "node": shape("text", "n1", 1.0, 1.0, json!({ "label": "T" })) }),
                json!({ "op": "remove-node", "id": "n1" }),
            ],
        ),
    ]
}

fn build() -> Value {
    let mut cases = Vec::new();
    for (name, ops) in op_cases() {
        let mut doc = FigureDocument::new(1, 960.0, 540.0);
        let mut steps = Vec::new();
        for raw in &ops {
            let step = match FigureOp::from_value(raw.clone()).and_then(|op| doc.apply(&op)) {
                Ok(result) => {
                    json!({ "op": raw, "ok": true, "changed": result.changed, "scene": doc.scene })
                }
                Err(_) => json!({ "op": raw, "ok": false, "scene": doc.scene }),
            };
            steps.push(step);
        }
        cases.push(json!({ "name": name, "steps": steps }));
    }

    let r = Rect::new(10.0, 20.0, 120.0, 60.0);
    let mut outlines = Vec::new();
    for roughness in 0..3u8 {
        for kind in ["rect", "diamond", "ellipse"] {
            let mut rng = Rng::for_id("parity");
            let d = match kind {
                "rect" => sketch::polygon(&sketch::rect_points(&r), roughness, &mut rng),
                "diamond" => sketch::polygon(&sketch::diamond_points(&r), roughness, &mut rng),
                _ => sketch::ellipse(&r, roughness, &mut rng),
            };
            outlines.push(json!({ "kind": kind, "seed": "parity", "rect": r, "roughness": roughness, "d": d }));
        }
    }
    let mut connectors = Vec::new();
    let pts = [
        manang_figure::Vec2::new(0.0, 0.0),
        manang_figure::Vec2::new(80.0, 40.0),
        manang_figure::Vec2::new(160.0, 0.0),
    ];
    for roughness in 0..3u8 {
        for routing in [
            manang_figure::Routing::Straight,
            manang_figure::Routing::Curved,
        ] {
            for n in [2usize, 3] {
                let mut rng = Rng::for_id("conn");
                let points: Vec<_> = if n == 2 {
                    vec![pts[0], pts[2]]
                } else {
                    pts.to_vec()
                };
                let ds = Some(manang_figure::Vec2::new(1.0, 0.0));
                let (d, sd, ed) =
                    sketch::connector(&points, routing, ds, None, roughness, &mut rng);
                connectors.push(
                    json!({ "points": points, "routing": routing, "ds": ds, "de": null,
                    "roughness": roughness, "seed": "conn", "d": d, "startDir": sd, "endDir": ed }),
                );
            }
        }
    }
    let mut seeds = Vec::new();
    for id in ["n1", "n2", "parity", "é→ü", ""] {
        let mut rng = Rng::for_id(id);
        let values: Vec<f64> = (0..5).map(|_| rng.unit()).collect();
        seeds.push(json!({ "id": id, "values": values }));
    }
    let mut wraps = Vec::new();
    for (text, width, size, family) in [
        (
            "A box with a long label that wraps",
            104.0,
            16.0,
            FontFamily::Sans,
        ),
        ("Supercalifragilistic", 60.0, 14.0, FontFamily::Sans),
        ("mono text here", 50.0, 12.0, FontFamily::Mono),
        ("line one\nline two is longer", 80.0, 16.0, FontFamily::Sans),
        ("", 80.0, 16.0, FontFamily::Sans),
    ] {
        wraps.push(
            json!({ "text": text, "width": width, "size": size, "family": family,
            "lines": wrap(text, width, size, family) }),
        );
    }
    // Routing against a fixed scene with a rotated shape.
    let mut scene: FigureScene = serde_json::from_value(json!({
        "version": 2, "width": 800, "height": 600, "nodes": [
            shape("rect", "a", 0.0, 0.0, json!({})),
            shape("ellipse", "b", 300.0, 150.0, json!({ "angle": 30 })),
            shape("diamond", "c", 0.0, 300.0, json!({})),
        ]
    }))
    .unwrap_or_else(|_| FigureScene::new(1.0, 1.0));
    let mut routes = Vec::new();
    for (routing, start, end) in [
        ("straight", json!({ "node": "a" }), json!({ "node": "b" })),
        ("elbow", json!({ "node": "a" }), json!({ "node": "c" })),
        (
            "elbow",
            json!({ "node": "a", "anchor": { "x": 1, "y": 1 } }),
            json!({ "node": "b" }),
        ),
        (
            "curved",
            json!({ "node": "c" }),
            json!({ "node": "b", "anchor": { "x": 0, "y": 0.5 } }),
        ),
        ("straight", json!({ "node": "a" }), json!({ "node": "a" })),
    ] {
        let conn: FigureNode = match serde_json::from_value(json!({ "kind": "arrow", "id": "x",
            "points": [{ "x": 0, "y": 0 }, { "x": 50, "y": 50 }, { "x": 99, "y": 99 }],
            "start": start, "end": end, "routing": routing }))
        {
            Ok(n) => n,
            Err(_) => continue,
        };
        if let Some(c) = conn.connector() {
            routes.push(json!({ "connector": conn, "points": route(&scene, c) }));
        }
    }
    scene.nodes.clear();
    json!({
        "about": "Generated by crates/manang-figure/tests/parity.rs; do not edit by hand.",
        "cases": cases,
        "outlines": outlines,
        "connectors": connectors,
        "seeds": seeds,
        "wraps": wraps,
        "routes": { "scene": serde_json::from_value::<Value>(json!({
            "version": 2, "width": 800, "height": 600, "nodes": [
                shape("rect", "a", 0.0, 0.0, json!({})),
                shape("ellipse", "b", 300.0, 150.0, json!({ "angle": 30 })),
                shape("diamond", "c", 0.0, 300.0, json!({})),
            ]})).unwrap_or(Value::Null), "cases": routes },
    })
}

#[test]
fn parity_fixture_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/parity.json");
    let mut text = serde_json::to_string_pretty(&build()).expect("serialises");
    text.push('\n');
    if std::env::var_os("UPDATE_PARITY").is_some() {
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
            .expect("dir");
        std::fs::write(path, &text).expect("writes");
        return;
    }
    let current = std::fs::read_to_string(path).expect("run with UPDATE_PARITY=1 to create it");
    assert!(
        current == text,
        "parity.json is stale: run UPDATE_PARITY=1 cargo test -p manang-figure --test parity"
    );
}
