use manang_figure::{
    ArrowNode, FigureNode, FigureScene, MathNode, RectNode, StrokeNode, StrokeSample, Vec2,
    format_node_id,
};

/// Constructs the canonical frontend sample figure from `src/lib/state/ink.svelte.ts`.
fn frontend_seed_scene() -> FigureScene {
    let mut scene = FigureScene::new(960.0, 540.0);

    let math = FigureNode::Math(MathNode {
        id: format_node_id(4),
        x: 120.0,
        y: 120.0,
        tex: "|0⟩  ⟶  |1⟩".to_string(),
        size: 28.0,
        color: "#b3d17a".to_string(),
    });

    let left = FigureNode::Rect(RectNode {
        id: format_node_id(1),
        x: 120.0,
        y: 200.0,
        width: 190.0,
        height: 84.0,
        stroke: "#e8e6e1".to_string(),
        fill: Some("#1c1c20".to_string()),
    });

    let arrow = FigureNode::Arrow(ArrowNode {
        id: format_node_id(3),
        from: Vec2::new(310.0, 242.0),
        to: Vec2::new(640.0, 242.0),
        color: "#c6f135".to_string(),
        label: Some("H".to_string()),
    });

    let right = FigureNode::Rect(RectNode {
        id: format_node_id(2),
        x: 640.0,
        y: 200.0,
        width: 190.0,
        height: 84.0,
        stroke: "#e8e6e1".to_string(),
        fill: Some("#1c1c20".to_string()),
    });

    let stroke = FigureNode::Stroke(StrokeNode {
        id: format_node_id(5),
        points: vec![
            StrokeSample::new(140.0, 340.0, 0.4),
            StrokeSample::new(180.0, 345.0, 0.6),
            StrokeSample::new(220.0, 335.0, 0.5),
            StrokeSample::new(260.0, 340.0, 0.7),
        ],
        size: 3.5,
        color: "#c6f135".to_string(),
    });

    scene.nodes = vec![math, left, arrow, right, stroke];
    scene
}

#[test]
fn test_frontend_sample_figure_svg_snapshot() {
    let scene = frontend_seed_scene();
    let svg = scene.to_svg();

    // Verify deterministic self-equality
    assert_eq!(svg, scene.to_svg());

    // Regression snapshot validation
    insta::assert_snapshot!(svg);
}
