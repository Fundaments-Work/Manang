use std::time::Instant;

use manang_figure::{
    FigureError, FigureNode, FigureScene, ShapeNode, SvgExportOptions, node_rect, render_svg,
};
use serde_json::json;

#[test]
fn test_empty_and_truncated_json() {
    let empty_inputs = [
        "",
        "   ",
        "null",
        "[]",
        "{}",
        r#"{"version": 2}"#,
        r#"{"version": 2, "nodes": ["#,
        r#"{"version": 2, "width": 800, "height": 600, "nodes": [{"id": "r1", "kind": "rect""#,
    ];

    for input in empty_inputs {
        let res = FigureScene::from_json(input);
        assert!(res.is_err(), "Expected error for input: {:?}", input);
    }
}

#[test]
fn test_duplicate_node_ids() {
    let fixture = include_str!("../../../testdata/robustness/figure/duplicate_ids.figure.json");
    let res = FigureScene::from_json(fixture);
    match res {
        Err(FigureError::DuplicateNode(id)) => {
            assert_eq!(id, "dup_node");
        }
        other => panic!("Expected DuplicateNode error, got {:?}", other),
    }
}

#[test]
fn test_nan_infinity_and_negative_sizes() {
    // 1. Invalid page sizes (negative, zero, NaN)
    let bad_pages = [
        (-800.0, 600.0),
        (800.0, -600.0),
        (0.0, 600.0),
        (800.0, 0.0),
        (f64::NAN, 600.0),
        (800.0, f64::INFINITY),
    ];
    for (w, h) in bad_pages {
        let scene = FigureScene::new(w, h);
        let val_res = scene.validate();
        assert!(
            val_res.is_err(),
            "Expected error for page size ({}, {})",
            w,
            h
        );
    }

    // 2. Node with non-finite numbers
    let mut scene = FigureScene::new(800.0, 600.0);
    scene.append_node(FigureNode::Rect(ShapeNode::new(
        "r1",
        0.0,
        0.0,
        f64::NAN,
        50.0,
    )));
    let val_res = scene.validate();
    assert!(val_res.is_err());

    // 3. Negative sizes in v2 nodes: geometry::node_rect normalizes safely
    let neg_node = FigureNode::Rect(ShapeNode::new("neg1", 100.0, 100.0, -50.0, -30.0));
    let r = node_rect(&neg_node);
    assert_eq!(r.x, 50.0);
    assert_eq!(r.y, 70.0);
    assert_eq!(r.width, 50.0);
    assert_eq!(r.height, 30.0);

    // SVG export on negative sized node
    let mut neg_scene = FigureScene::new(800.0, 600.0);
    neg_scene.append_node(neg_node);
    let svg = render_svg(&neg_scene, &SvgExportOptions::default());
    assert!(svg.contains("<svg"));
}

#[test]
fn test_unknown_node_kinds_and_fields() {
    // Unknown top-level field on FigureScene
    let unknown_scene_field = json!({
        "version": 2,
        "width": 800,
        "height": 600,
        "unrecognized_meta": "surprise",
        "nodes": []
    });
    let res = FigureScene::from_value(unknown_scene_field);
    assert!(res.is_err());

    // Unknown node kind
    let unknown_kind = json!({
        "version": 2,
        "width": 800,
        "height": 600,
        "nodes": [
            {
                "kind": "custom_gizmo_3000",
                "id": "g1",
                "x": 10,
                "y": 10
            }
        ]
    });
    let res = FigureScene::from_value(unknown_kind);
    assert!(res.is_err());

    // Unknown field inside known node
    let unknown_node_field = json!({
        "version": 2,
        "width": 800,
        "height": 600,
        "nodes": [
            {
                "kind": "rect",
                "id": "r1",
                "x": 10,
                "y": 10,
                "width": 100,
                "height": 100,
                "extra_alien_field": 42
            }
        ]
    });
    let res = FigureScene::from_value(unknown_node_field);
    assert!(res.is_err());
}

#[test]
fn test_v1_migration_all_features() {
    let fixture = include_str!("../../../testdata/robustness/figure/v1_all_features.figure.json");
    let scene = FigureScene::from_json(fixture).expect("v1 migration succeeds");

    assert_eq!(scene.version, 2);
    assert_eq!(scene.nodes.len(), 5);

    // Node 0: rect (originally x: 100, y: 100, width: -150, height: -80)
    let FigureNode::Rect(r) = &scene.nodes[0] else {
        panic!("expected rect")
    };
    assert_eq!(r.id, "r1");
    assert_eq!(r.x, -50.0);
    assert_eq!(r.y, 20.0);
    assert_eq!(r.width, 150.0);
    assert_eq!(r.height, 80.0);
    assert_eq!(r.style.stroke.as_deref(), Some("crimson"));
    assert_eq!(r.style.fill.as_deref(), Some("lemon"));

    // Node 1: ellipse (originally cx: 300, cy: 200, rx: -50, ry: 30)
    let FigureNode::Ellipse(e) = &scene.nodes[1] else {
        panic!("expected ellipse")
    };
    assert_eq!(e.id, "e1");
    assert_eq!(e.x, 250.0);
    assert_eq!(e.y, 170.0);
    assert_eq!(e.width, 100.0);
    assert_eq!(e.height, 60.0);
    assert_eq!(e.style.stroke.as_deref(), Some("#336699"));
    assert_eq!(e.style.fill.as_deref(), Some("lavender"));

    // Node 2: freedraw (originally stroke)
    let FigureNode::Freedraw(f) = &scene.nodes[2] else {
        panic!("expected freedraw")
    };
    assert_eq!(f.id, "s1");
    assert_eq!(f.size, 3.0);
    assert_eq!(f.points.len(), 3);
    assert_eq!(f.style.stroke.as_deref(), Some("ink"));

    // Node 3: arrow (originally arrow with from, to)
    let FigureNode::Arrow(a) = &scene.nodes[3] else {
        panic!("expected arrow")
    };
    assert_eq!(a.id, "a1");
    assert_eq!(a.points.len(), 2);
    assert_eq!(a.points[0].x, 50.0);
    assert_eq!(a.points[0].y, 50.0);
    assert_eq!(a.points[1].x, 200.0);
    assert_eq!(a.points[1].y, 180.0);
    assert_eq!(a.label.as_deref(), Some("v1 arrow"));
    assert_eq!(a.style.stroke.as_deref(), Some("pine"));

    // Node 4: math (originally tex "E = mc^2", y: 350, size: 24)
    let FigureNode::Math(m) = &scene.nodes[4] else {
        panic!("expected math")
    };
    assert_eq!(m.id, "m1");
    assert_eq!(m.source, "E = mc^2");
    assert_eq!(m.language.as_str(), "latex");
    assert_eq!(m.size, 24.0);
    assert_eq!(m.y, 326.0); // 350 - 24

    // Render SVG for migrated v1 scene
    let svg = render_svg(&scene, &SvgExportOptions::default());
    assert!(svg.contains("<svg"));
    assert!(svg.contains("v1 arrow"));

    // Round-trip serialization
    let file_json = scene.to_file_json().expect("to_file_json");
    let reloaded = FigureScene::from_json(&file_json).expect("reload");
    assert_eq!(reloaded.nodes.len(), 5);
}

#[test]
fn test_huge_node_count_100k_performance() {
    let mut scene = FigureScene::new(50_000.0, 50_000.0);
    let count = 100_000;

    println!("\n[figure::robustness] Generating {} nodes...", count);
    let t0 = Instant::now();
    for i in 0..count {
        let x = (i % 500) as f64 * 60.0;
        let y = (i / 500) as f64 * 40.0;
        let node = FigureNode::Rect(ShapeNode::new(format!("n{}", i), x, y, 50.0, 30.0));
        scene.append_node(node);
    }
    let gen_time = t0.elapsed();
    println!("  Generated in: {:?}", gen_time);

    // 1. Serialization to compact JSON
    let t0 = Instant::now();
    let json_str = scene.to_json().expect("serialize 100k nodes");
    let ser_time = t0.elapsed();
    let json_mb = json_str.len() as f64 / (1024.0 * 1024.0);
    println!(
        "  Serialized to JSON ({:.2} MB) in: {:?}",
        json_mb, ser_time
    );

    // 2. Deserialization from JSON
    let t0 = Instant::now();
    let loaded = FigureScene::from_json(&json_str).expect("deserialize 100k nodes");
    let de_time = t0.elapsed();
    println!("  Deserialized 100k nodes in: {:?}", de_time);
    assert_eq!(loaded.nodes.len(), count);

    // 3. SVG rendering of 100k nodes
    let t0 = Instant::now();
    let svg = render_svg(&loaded, &SvgExportOptions::default());
    let svg_time = t0.elapsed();
    let svg_mb = svg.len() as f64 / (1024.0 * 1024.0);
    println!("  Rendered SVG ({:.2} MB) in: {:?}", svg_mb, svg_time);
    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>\n"));
}
