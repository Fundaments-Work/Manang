use manang_figure::{
    ArrowNode, EllipseNode, FigureNode, FigureScene, MathNode, RectNode, StrokeNode, StrokeSample,
    Vec2,
};
use proptest::prelude::*;

fn arbitrary_coord() -> impl Strategy<Value = f64> {
    (-1000.0..1000.0f64).prop_map(|v| (v * 10.0).round() / 10.0)
}

fn arbitrary_size() -> impl Strategy<Value = f64> {
    (0.1..100.0f64).prop_map(|v| (v * 10.0).round() / 10.0)
}

fn arbitrary_color() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("#141416".to_string()),
        Just("#1c1c20".to_string()),
        Just("#e8e6e1".to_string()),
        Just("#c6f135".to_string()),
        Just("#b3d17a".to_string()),
        Just("#3d3d42".to_string()),
    ]
}

fn arbitrary_stroke_sample() -> impl Strategy<Value = StrokeSample> {
    (arbitrary_coord(), arbitrary_coord(), 0.0..1.0f64)
        .prop_map(|(x, y, p)| StrokeSample::new(x, y, (p * 100.0).round() / 100.0))
}

fn arbitrary_figure_node() -> impl Strategy<Value = FigureNode> {
    prop_oneof![
        (
            "[a-z0-9_]{1,8}",
            proptest::collection::vec(arbitrary_stroke_sample(), 0..10),
            arbitrary_size(),
            arbitrary_color()
        )
            .prop_map(|(id, points, size, color)| {
                FigureNode::Stroke(StrokeNode {
                    id,
                    points,
                    size,
                    color,
                })
            }),
        (
            "[a-z0-9_]{1,8}",
            arbitrary_coord(),
            arbitrary_coord(),
            arbitrary_size(),
            arbitrary_size(),
            arbitrary_color(),
            proptest::option::of(arbitrary_color())
        )
            .prop_map(|(id, x, y, width, height, stroke, fill)| {
                FigureNode::Rect(RectNode {
                    id,
                    x,
                    y,
                    width,
                    height,
                    stroke,
                    fill,
                })
            }),
        (
            "[a-z0-9_]{1,8}",
            arbitrary_coord(),
            arbitrary_coord(),
            arbitrary_size(),
            arbitrary_size(),
            arbitrary_color(),
            proptest::option::of(arbitrary_color())
        )
            .prop_map(|(id, cx, cy, rx, ry, stroke, fill)| {
                FigureNode::Ellipse(EllipseNode {
                    id,
                    cx,
                    cy,
                    rx,
                    ry,
                    stroke,
                    fill,
                })
            }),
        (
            "[a-z0-9_]{1,8}",
            arbitrary_coord(),
            arbitrary_coord(),
            arbitrary_coord(),
            arbitrary_coord(),
            arbitrary_color(),
            proptest::option::of("[a-zA-Z0-9 ]{1,10}")
        )
            .prop_map(|(id, fx, fy, tx, ty, color, label)| {
                FigureNode::Arrow(ArrowNode {
                    id,
                    from: Vec2::new(fx, fy),
                    to: Vec2::new(tx, ty),
                    color,
                    label,
                })
            }),
        (
            "[a-z0-9_]{1,8}",
            arbitrary_coord(),
            arbitrary_coord(),
            "[a-zA-Z0-9+\\-*/= ]{1,15}",
            arbitrary_size(),
            arbitrary_color()
        )
            .prop_map(|(id, x, y, tex, size, color)| {
                FigureNode::Math(MathNode {
                    id,
                    x,
                    y,
                    tex,
                    size,
                    color,
                })
            }),
    ]
}

fn arbitrary_figure_scene() -> impl Strategy<Value = FigureScene> {
    (
        arbitrary_size(),
        arbitrary_size(),
        proptest::collection::vec(arbitrary_figure_node(), 0..15),
    )
        .prop_map(|(w, h, nodes)| {
            let mut scene = FigureScene::new(w, h);
            scene.nodes = nodes;
            scene
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn test_json_round_trip(scene in arbitrary_figure_scene()) {
        let serialized = scene.to_json().expect("serialization must succeed");
        let deserialized: FigureScene = FigureScene::from_json(&serialized)
            .expect("deserialization must succeed");

        prop_assert_eq!(scene, deserialized);
    }
}
