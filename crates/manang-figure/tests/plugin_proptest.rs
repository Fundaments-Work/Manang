//! Property tests for the figure plugin: random operations never panic,
//! and undo/redo faithfully round-trips scene state.

use manang_figure::{
    ArrowNode, EllipseNode, FigureDocument, FigureNode, FigureOp, MathNode, RectNode, StrokeNode,
    StrokeSample, Vec2,
};
use proptest::prelude::*;

fn arb_vec2() -> impl Strategy<Value = Vec2> {
    (-1000.0..1000.0f64, -1000.0..1000.0f64).prop_map(|(x, y)| Vec2::new(x, y))
}

fn arb_stroke_sample() -> impl Strategy<Value = StrokeSample> {
    (-500.0..500.0f64, -500.0..500.0f64, 0.0..1.0f64).prop_map(|(x, y, pressure)| StrokeSample {
        x,
        y,
        pressure,
    })
}

fn arb_rect_node() -> impl Strategy<Value = RectNode> {
    (
        "[a-z0-9_]{1,10}",
        -500.0..500.0f64,
        -500.0..500.0f64,
        0.0..500.0f64,
        0.0..500.0f64,
        "#[0-9a-f]{6}",
        proptest::option::of("#[0-9a-f]{6}"),
    )
        .prop_map(|(id, x, y, width, height, stroke, fill)| RectNode {
            id,
            x,
            y,
            width,
            height,
            stroke,
            fill,
        })
}

fn arb_ellipse_node() -> impl Strategy<Value = EllipseNode> {
    (
        "[a-z0-9_]{1,10}",
        -500.0..500.0f64,
        -500.0..500.0f64,
        0.0..500.0f64,
        0.0..500.0f64,
        "#[0-9a-f]{6}",
        proptest::option::of("#[0-9a-f]{6}"),
    )
        .prop_map(|(id, cx, cy, rx, ry, stroke, fill)| EllipseNode {
            id,
            cx,
            cy,
            rx,
            ry,
            stroke,
            fill,
        })
}

fn arb_arrow_node() -> impl Strategy<Value = ArrowNode> {
    (
        "[a-z0-9_]{1,10}",
        arb_vec2(),
        arb_vec2(),
        "#[0-9a-f]{6}",
        proptest::option::of("[a-zA-Z0-9 ]{1,20}"),
    )
        .prop_map(|(id, from, to, color, label)| ArrowNode {
            id,
            from,
            to,
            color,
            label,
        })
}

fn arb_math_node() -> impl Strategy<Value = MathNode> {
    (
        "[a-z0-9_]{1,10}",
        -500.0..500.0f64,
        -500.0..500.0f64,
        "[a-zA-Z0-9+\\-*/= ]{1,20}",
        8.0..48.0f64,
        "#[0-9a-f]{6}",
    )
        .prop_map(|(id, x, y, tex, size, color)| MathNode {
            id,
            x,
            y,
            tex,
            size,
            color,
        })
}

fn arb_stroke_node() -> impl Strategy<Value = StrokeNode> {
    (
        "[a-z0-9_]{1,10}",
        proptest::collection::vec(arb_stroke_sample(), 0..10),
        1.0..20.0f64,
        "#[0-9a-f]{6}",
    )
        .prop_map(|(id, points, size, color)| StrokeNode {
            id,
            points,
            size,
            color,
        })
}

fn arb_node() -> impl Strategy<Value = FigureNode> {
    prop_oneof![
        arb_rect_node().prop_map(FigureNode::Rect),
        arb_ellipse_node().prop_map(FigureNode::Ellipse),
        arb_arrow_node().prop_map(FigureNode::Arrow),
        arb_math_node().prop_map(FigureNode::Math),
        arb_stroke_node().prop_map(FigureNode::Stroke),
    ]
}

fn arb_op() -> impl Strategy<Value = FigureOp> {
    prop_oneof![
        arb_node().prop_map(|node| FigureOp::AddNode { node }),
        arb_node().prop_map(|node| FigureOp::UpdateNode { node }),
        "[a-z0-9_]{1,10}".prop_map(|id| FigureOp::RemoveNode { id }),
        Just(FigureOp::Clear),
        (
            "[a-z0-9_]{1,10}",
            proptest::option::of("#[0-9a-f]{6}"),
            proptest::option::of("#[0-9a-f]{6}"),
            proptest::option::of(1.0..32.0f64),
            proptest::option::of("#[0-9a-f]{6}")
        )
            .prop_map(|(id, stroke, fill, size, color)| FigureOp::SetStyle {
                id,
                stroke,
                fill,
                size,
                color,
            }),
        ("[a-z0-9_]{1,10}", -50.0..50.0f64, -50.0..50.0f64)
            .prop_map(|(id, dx, dy)| FigureOp::Translate { id, dx, dy }),
        Just(FigureOp::Undo),
        Just(FigureOp::Redo),
    ]
}

proptest! {
    /// Arbitrary op sequences applied to a figure document must never panic.
    #[test]
    fn random_ops_never_panic(ops in proptest::collection::vec(arb_op(), 1..60)) {
        let mut doc = FigureDocument::new(1, 800.0, 600.0);
        let mut rev = 0;
        for op in ops {
            if let Ok(res) = doc.apply(op) {
                prop_assert!(res.revision >= rev);
                rev = res.revision;
            }
        }
    }

    /// Adding a node, then undoing, must restore the exact previous scene JSON,
    /// and redoing must restore the added state.
    #[test]
    fn undo_redo_round_trips_scene(node in arb_node()) {
        let mut doc = FigureDocument::new(1, 800.0, 600.0);
        let before_json = doc.scene.to_json().expect("valid json before");

        let add_res = doc.apply(FigureOp::AddNode { node: node.clone() }).expect("add");
        prop_assert_eq!(add_res.applied, true);
        let after_json = doc.scene.to_json().expect("valid json after");

        // Undo
        let undo_res = doc.apply(FigureOp::Undo).expect("undo");
        prop_assert_eq!(undo_res.applied, true);
        let restored_json = doc.scene.to_json().expect("valid json restored");
        prop_assert_eq!(&restored_json, &before_json);

        // Redo
        let redo_res = doc.apply(FigureOp::Redo).expect("redo");
        prop_assert_eq!(redo_res.applied, true);
        let redone_json = doc.scene.to_json().expect("valid json redone");
        prop_assert_eq!(&redone_json, &after_json);

        // Undo again
        let undo2 = doc.apply(FigureOp::Undo).expect("undo 2");
        prop_assert_eq!(undo2.applied, true);
        let restored_json2 = doc.scene.to_json().expect("valid json restored 2");
        prop_assert_eq!(&restored_json2, &before_json);
    }

    /// Moving a node is one undo step: the node moves exactly by the delta,
    /// keeps its identity, and undo restores the exact previous scene.
    #[test]
    fn translate_is_one_exact_undo_step(
        node in arb_node(),
        dx in -100.0..100.0f64,
        dy in -100.0..100.0f64,
    ) {
        let mut doc = FigureDocument::new(1, 800.0, 600.0);
        doc.apply(FigureOp::AddNode { node: node.clone() }).expect("add");
        let before = doc.scene.clone();
        let res = doc
            .apply(FigureOp::Translate { id: node.id().to_owned(), dx, dy })
            .expect("translate");
        prop_assert_eq!(res.applied, dx != 0.0 || dy != 0.0);
        let moved = doc.scene.node_by_id(node.id()).expect("still there").clone();
        let mut expected = node.clone();
        if res.applied {
            expected.translate(dx, dy);
        }
        prop_assert_eq!(&moved, &expected);
        if res.applied {
            prop_assert!(res.can_undo);
            doc.apply(FigureOp::Undo).expect("undo");
            prop_assert_eq!(&doc.scene, &before);
        }
    }
}
