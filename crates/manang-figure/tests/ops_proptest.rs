//! Property tests for the scene model and its operations (AGENTS.md 4.1):
//! random operation sequences never panic, a failed operation changes
//! nothing, undo restores the scene exactly and redo brings the change back,
//! and scenes survive the file round trip.

use manang_figure::{
    AlignEdge, Axis, Binding, ConnectorNode, End, FigureDocument, FigureNode, FigureOp,
    FigureScene, FreedrawNode, MathLanguage, MathNode, Routing, ShapeNode, StrokeSample, Style,
    Vec2, ZOrder,
};
use proptest::prelude::*;
use serde_json::{Map, Value, json};

const IDS: [&str; 8] = ["n1", "n2", "n3", "n4", "n5", "n6", "n7", "ghost"];

fn coord() -> impl Strategy<Value = f64> {
    (-50_000i32..50_000).prop_map(|k| f64::from(k) / 100.0)
}

fn size() -> impl Strategy<Value = f64> {
    (100i32..40_000).prop_map(|k| f64::from(k) / 100.0)
}

fn id() -> impl Strategy<Value = String> {
    prop::sample::select(IDS.to_vec()).prop_map(str::to_owned)
}

fn ids() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(id(), 1..4)
}

fn style() -> impl Strategy<Value = Style> {
    (
        prop::option::of(prop::sample::select(vec![
            "ink", "blue", "#ff8800", "accent",
        ])),
        prop::option::of(prop::sample::select(vec!["none", "surface", "orange"])),
        prop::option::of(0u8..3),
        prop::option::of(1u8..5),
        prop::option::of(prop::sample::select(vec!["sage", "#0a1b2c"])),
        prop::option::of(0u8..=10),
    )
        .prop_map(|(stroke, fill, roughness, w, label, fill_opacity)| Style {
            stroke: stroke.map(str::to_owned),
            fill: fill.map(str::to_owned),
            roughness,
            stroke_width: w.map(f64::from),
            label_color: label.map(str::to_owned),
            fill_opacity: fill_opacity.map(|p| f64::from(p) * 10.0),
            ..Style::default()
        })
}

fn shape(id: String) -> impl Strategy<Value = FigureNode> {
    (
        coord(),
        coord(),
        size(),
        size(),
        prop::option::of("[a-z ]{0,12}"),
        style(),
        0usize..5,
        prop::sample::select(vec![0.0, 0.0, 30.0, 90.0]),
    )
        .prop_map(move |(x, y, w, h, label, style, kind, angle)| {
            let mut s = ShapeNode::new(id.clone(), x, y, w, h);
            s.label = label;
            s.style = style;
            s.angle = angle;
            match kind {
                0 => FigureNode::Rect(s),
                1 => FigureNode::Diamond(s),
                2 => FigureNode::Ellipse(s),
                3 => FigureNode::Text(s),
                _ => FigureNode::Frame(s),
            }
        })
}

fn connector(id: String) -> impl Strategy<Value = FigureNode> {
    (
        coord(),
        coord(),
        coord(),
        coord(),
        prop::option::of(prop::sample::select(IDS.to_vec())),
        prop::option::of(prop::sample::select(IDS.to_vec())),
        prop::sample::select(vec![Routing::Straight, Routing::Elbow, Routing::Curved]),
        any::<bool>(),
    )
        .prop_map(move |(x1, y1, x2, y2, s, e, routing, arrow)| {
            let mut c = ConnectorNode::new(id.clone(), Vec2::new(x1, y1), Vec2::new(x2, y2));
            c.start = s.map(Binding::to);
            c.end = e.map(Binding::to);
            c.routing = routing;
            if arrow {
                FigureNode::Arrow(c)
            } else {
                FigureNode::Line(c)
            }
        })
}

fn freedraw(id: String) -> impl Strategy<Value = FigureNode> {
    prop::collection::vec((coord(), coord()), 1..6).prop_map(move |pts| {
        FigureNode::Freedraw(FreedrawNode {
            id: id.clone(),
            points: pts
                .into_iter()
                .map(|(x, y)| StrokeSample::new(x, y, 0.5))
                .collect(),
            size: 2.5,
            style: Style::default(),
            groups: Vec::new(),
            locked: false,
            hidden: false,
        })
    })
}

fn math(id: String) -> impl Strategy<Value = FigureNode> {
    (
        coord(),
        coord(),
        "[a-z^_ ]{1,10}",
        any::<bool>(),
        size(),
        style(),
    )
        .prop_map(move |(x, y, source, latex, measured, style)| {
            let language = if latex {
                MathLanguage::Latex
            } else {
                MathLanguage::Typst
            };
            let mut m = MathNode::new(id.clone(), x, y, source, language, 18.0);
            m.width = measured.round();
            m.height = (measured / 2.0).round();
            m.style = style;
            FigureNode::Math(m)
        })
}

fn node() -> impl Strategy<Value = FigureNode> {
    id().prop_flat_map(|id| {
        prop_oneof![
            4 => shape(id.clone()),
            2 => connector(id.clone()),
            1 => freedraw(id.clone()),
            1 => math(id),
        ]
    })
}

fn update_set() -> impl Strategy<Value = Map<String, Value>> {
    prop_oneof![
        "[a-z]{0,8}".prop_map(|l| json!({ "label": l })),
        Just(json!({ "label": null })),
        Just(json!({ "kind": "diamond" })),
        Just(json!({ "style": { "stroke": "green", "fill": null } })),
        Just(json!({ "x": 10, "y": -20 })),
        Just(json!({ "routing": "elbow" })),
        Just(json!({ "id": "other" })),
        Just(json!({ "width": "wide" })),
    ]
    .prop_map(|v| v.as_object().cloned().unwrap_or_default())
}

fn op() -> impl Strategy<Value = FigureOp> {
    let leaf = prop_oneof![
        4 => prop::collection::vec(node(), 1..3).prop_map(|nodes| FigureOp::Add { nodes, index: None }),
        2 => (id(), update_set()).prop_map(|(id, set)| FigureOp::Update { id, set }),
        1 => ids().prop_map(|ids| FigureOp::Remove { ids }),
        3 => (ids(), coord(), coord()).prop_map(|(ids, dx, dy)| FigureOp::Move { ids, dx, dy }),
        1 => (id(), coord(), coord(), coord(), coord())
            .prop_map(|(id, x, y, width, height)| FigureOp::Resize { id, x, y, width, height }),
        1 => (ids(), -400.0f64..400.0).prop_map(|(ids, angle)| FigureOp::Rotate { ids, angle }),
        1 => (ids(), prop::sample::select(vec![ZOrder::Front, ZOrder::Back, ZOrder::Forward, ZOrder::Backward]))
            .prop_map(|(ids, to)| FigureOp::Reorder { ids, to }),
        1 => ids().prop_map(|ids| FigureOp::Group { ids, group: None }),
        1 => ids().prop_map(|ids| FigureOp::Ungroup { ids }),
        2 => (id(), any::<bool>(), prop::option::of(id()))
            .prop_map(|(id, start, node)| FigureOp::Bind {
                id,
                end: if start { End::Start } else { End::End },
                node,
                anchor: None,
            }),
        1 => (ids(), prop::sample::select(vec![AlignEdge::Left, AlignEdge::Center, AlignEdge::Bottom]))
            .prop_map(|(ids, edge)| FigureOp::Align { ids, edge }),
        1 => (ids(), any::<bool>()).prop_map(|(ids, h)| FigureOp::Distribute {
            ids,
            axis: if h { Axis::Horizontal } else { Axis::Vertical },
        }),
        1 => (ids(), style()).prop_map(|(ids, style)| FigureOp::SetStyle {
            ids,
            style: Some(style),
            id: None,
            stroke: None,
            fill: None,
            size: None,
            color: None,
        }),
        1 => (size(), size(), prop::option::of(prop::sample::select(vec!["none", "paper", "#102030"])))
            .prop_map(|(width, height, bg)| FigureOp::SetCanvas {
                width,
                height,
                background: bg.map(str::to_owned),
            }),
        1 => Just(FigureOp::Clear),
    ];
    leaf.prop_recursive(1, 8, 4, |inner| {
        prop::collection::vec(inner, 1..4).prop_map(|ops| FigureOp::Batch { ops })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn undo_restores_and_redo_reapplies_every_step(ops in prop::collection::vec(op(), 1..40)) {
        let mut doc = FigureDocument::new(1, 800.0, 600.0);
        for op in &ops {
            let before = doc.scene.clone();
            match doc.apply(op) {
                Ok(result) if result.applied => {
                    let after = doc.scene.clone();
                    prop_assert!(doc.scene.validate().is_ok(), "{:?}", doc.scene.validate());
                    prop_assert!(doc.apply(&FigureOp::Undo).expect("undo").applied);
                    prop_assert_eq!(&doc.scene, &before);
                    prop_assert!(doc.apply(&FigureOp::Redo).expect("redo").applied);
                    prop_assert_eq!(&doc.scene, &after);
                }
                Ok(_) => prop_assert_eq!(&doc.scene, &before),
                Err(_) => prop_assert_eq!(&doc.scene, &before, "a failed op changed the scene"),
            }
        }
        // Undoing everything gets back to the empty page.
        while doc.apply(&FigureOp::Undo).expect("undo").applied {}
        prop_assert!(doc.scene.nodes.is_empty());
        prop_assert_eq!((doc.scene.width, doc.scene.height), (800.0, 600.0));
    }

    #[test]
    fn ops_round_trip_through_json(op in op()) {
        let value = serde_json::to_value(&op).expect("serialises");
        let back = FigureOp::from_value(value).expect("parses");
        prop_assert_eq!(back, op);
    }

    #[test]
    fn scenes_round_trip_through_the_file_form(nodes in prop::collection::vec(node(), 0..8)) {
        let mut scene = FigureScene::new(960.0, 540.0);
        for n in nodes {
            if scene.node_by_id(n.id()).is_none() {
                scene.nodes.push(n);
            }
        }
        let text = scene.to_file_json().expect("writes");
        let back = FigureScene::from_json(&text).expect("reads");
        prop_assert_eq!(&back, &scene);
        prop_assert_eq!(back.to_file_json().expect("writes"), text);
        // Rendering never panics and is deterministic.
        prop_assert_eq!(back.to_svg(), scene.to_svg());
    }

    #[test]
    fn arbitrary_json_never_panics_the_reader(text in ".{0,200}") {
        let _ = FigureScene::from_json(&text);
        let _ = FigureOp::from_value(Value::String(text));
    }
}

#[test]
fn moving_a_shape_drags_its_bound_arrows_and_undo_puts_them_back() {
    let mut doc = FigureDocument::new(1, 800.0, 600.0);
    let a = FigureNode::Rect(ShapeNode::new("a", 0.0, 0.0, 100.0, 50.0));
    let b = FigureNode::Rect(ShapeNode::new("b", 300.0, 0.0, 100.0, 50.0));
    let mut c = ConnectorNode::new("c", Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
    c.start = Some(Binding::to("a"));
    c.end = Some(Binding::to("b"));
    doc.apply(&FigureOp::Add {
        nodes: vec![a, b, FigureNode::Arrow(c)],
        index: None,
    })
    .expect("adds");
    let arrow = |doc: &FigureDocument| {
        doc.scene
            .node_by_id("c")
            .and_then(FigureNode::connector)
            .map(|c| c.points.clone())
    };
    assert_eq!(
        arrow(&doc),
        Some(vec![Vec2::new(104.0, 25.0), Vec2::new(296.0, 25.0)])
    );
    let moved = doc
        .apply(&FigureOp::Move {
            ids: vec!["b".into()],
            dx: 0.0,
            dy: 200.0,
        })
        .expect("moves");
    assert_eq!(moved.changed, vec!["b".to_owned(), "c".to_owned()]);
    let routed = arrow(&doc).expect("arrow");
    assert!(routed[1].y > 150.0, "the end followed: {routed:?}");
    doc.apply(&FigureOp::Undo).expect("undo");
    assert_eq!(
        arrow(&doc),
        Some(vec![Vec2::new(104.0, 25.0), Vec2::new(296.0, 25.0)])
    );
}

#[test]
fn removing_a_shape_unbinds_its_arrows() {
    let mut doc = FigureDocument::new(1, 800.0, 600.0);
    let a = FigureNode::Rect(ShapeNode::new("a", 0.0, 0.0, 100.0, 50.0));
    let mut c = ConnectorNode::new("c", Vec2::new(0.0, 0.0), Vec2::new(300.0, 25.0));
    c.start = Some(Binding::to("a"));
    doc.apply(&FigureOp::Add {
        nodes: vec![a, FigureNode::Arrow(c)],
        index: None,
    })
    .expect("adds");
    doc.apply(&FigureOp::Remove {
        ids: vec!["a".into()],
    })
    .expect("removes");
    let c = doc
        .scene
        .node_by_id("c")
        .and_then(FigureNode::connector)
        .expect("arrow stays");
    assert!(c.start.is_none());
}

#[test]
fn align_and_distribute_line_shapes_up() {
    let mut doc = FigureDocument::new(1, 800.0, 600.0);
    let nodes = vec![
        FigureNode::Rect(ShapeNode::new("a", 0.0, 10.0, 50.0, 50.0)),
        FigureNode::Rect(ShapeNode::new("b", 100.0, 40.0, 50.0, 50.0)),
        FigureNode::Rect(ShapeNode::new("c", 400.0, 0.0, 50.0, 50.0)),
    ];
    doc.apply(&FigureOp::Add { nodes, index: None })
        .expect("adds");
    let ids: Vec<String> = ["a", "b", "c"].map(str::to_owned).to_vec();
    doc.apply(&FigureOp::Align {
        ids: ids.clone(),
        edge: AlignEdge::Top,
    })
    .expect("aligns");
    let ys: Vec<f64> = ids
        .iter()
        .filter_map(|i| doc.scene.node_by_id(i))
        .map(|n| manang_figure::node_rect(n).y)
        .collect();
    assert_eq!(ys, vec![0.0, 0.0, 0.0]);
    doc.apply(&FigureOp::Distribute {
        ids: ids.clone(),
        axis: Axis::Horizontal,
    })
    .expect("distributes");
    let xs: Vec<f64> = ids
        .iter()
        .filter_map(|i| doc.scene.node_by_id(i))
        .map(|n| manang_figure::node_rect(n).x)
        .collect();
    assert_eq!(xs, vec![0.0, 200.0, 400.0]);
}

#[test]
fn reorder_group_and_ungroup() {
    let mut doc = FigureDocument::new(1, 800.0, 600.0);
    let nodes = ["a", "b", "c"]
        .map(|i| FigureNode::Rect(ShapeNode::new(i, 0.0, 0.0, 10.0, 10.0)))
        .to_vec();
    doc.apply(&FigureOp::Add { nodes, index: None })
        .expect("adds");
    doc.apply(&FigureOp::Reorder {
        ids: vec!["a".into()],
        to: ZOrder::Front,
    })
    .expect("front");
    let order: Vec<&str> = doc.scene.nodes.iter().map(FigureNode::id).collect();
    assert_eq!(order, vec!["b", "c", "a"]);
    doc.apply(&FigureOp::Group {
        ids: vec!["a".into(), "b".into()],
        group: None,
    })
    .expect("group");
    assert_eq!(
        doc.scene.node_by_id("a").map(|n| n.groups().to_vec()),
        Some(vec!["g1".to_owned()])
    );
    doc.apply(&FigureOp::Group {
        ids: vec!["a".into(), "c".into()],
        group: None,
    })
    .expect("nest");
    assert_eq!(
        doc.scene.node_by_id("a").map(|n| n.groups().to_vec()),
        Some(vec!["g1".to_owned(), "g2".to_owned()])
    );
    doc.apply(&FigureOp::Ungroup {
        ids: vec!["a".into(), "c".into()],
    })
    .expect("ungroup");
    assert_eq!(
        doc.scene.node_by_id("a").map(|n| n.groups().to_vec()),
        Some(vec!["g1".to_owned()])
    );
}

#[test]
fn bad_colours_are_refused_with_the_palette() {
    let mut doc = FigureDocument::new(1, 800.0, 600.0);
    let mut s = ShapeNode::new("a", 0.0, 0.0, 10.0, 10.0);
    s.style.stroke = Some("red".into());
    let err = doc
        .apply(&FigureOp::Add {
            nodes: vec![FigureNode::Rect(s)],
            index: None,
        })
        .expect_err("refused");
    assert!(err.to_string().contains("vermillion"), "{err}");
    assert!(doc.scene.nodes.is_empty());
}
