use manang_figure::{
    AnchorSide, EllipseNode, FigureNode, FigureScene, Rect, RectNode, SNAP_DISTANCE, StrokeNode,
    StrokeSample, Vec2, anchors_of, arrow_head, format_node_id, hit_test, node_bounds,
    normalize_rect, polygon_to_path, scene_bounds, snap_to_anchors, stroke_to_path, union_rect,
};

fn rect_at(x: f64, y: f64, width: f64, height: f64) -> FigureNode {
    FigureNode::Rect(RectNode {
        id: "r1".to_string(),
        x,
        y,
        width,
        height,
        stroke: "#e8e6e1".to_string(),
        fill: None,
    })
}

#[test]
fn test_node_bounds_rect() {
    let rect = rect_at(10.0, 20.0, 30.0, 40.0);
    assert_eq!(
        node_bounds(&rect),
        Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        }
    );
}

#[test]
fn test_node_bounds_ellipse() {
    let ellipse = FigureNode::Ellipse(EllipseNode {
        id: "e1".to_string(),
        cx: 50.0,
        cy: 50.0,
        rx: 20.0,
        ry: 10.0,
        stroke: "#e8e6e1".to_string(),
        fill: None,
    });
    assert_eq!(
        node_bounds(&ellipse),
        Rect {
            x: 30.0,
            y: 40.0,
            width: 40.0,
            height: 20.0,
        }
    );
}

#[test]
fn test_node_bounds_stroke() {
    let stroke = FigureNode::Stroke(StrokeNode {
        id: "s1".to_string(),
        points: vec![
            StrokeSample::new(10.0, 10.0, 0.5),
            StrokeSample::new(30.0, 10.0, 0.5),
        ],
        size: 4.0,
        color: "#e8e6e1".to_string(),
    });
    let bounds = node_bounds(&stroke);
    assert_eq!(bounds.x, 8.0);
    assert_eq!(bounds.width, 24.0);
}

#[test]
fn test_node_bounds_empty_stroke() {
    let stroke = FigureNode::Stroke(StrokeNode {
        id: "s1".to_string(),
        points: vec![],
        size: 4.0,
        color: "#e8e6e1".to_string(),
    });
    assert_eq!(node_bounds(&stroke), Rect::EMPTY);
}

#[test]
fn test_normalize_rect() {
    assert_eq!(
        normalize_rect(100.0, 100.0, -40.0, -20.0),
        Rect {
            x: 60.0,
            y: 80.0,
            width: 40.0,
            height: 20.0,
        }
    );
}

#[test]
fn test_union_rect() {
    let r1 = Rect::new(5.0, 5.0, 10.0, 10.0);
    assert_eq!(union_rect(&Rect::EMPTY, &r1), r1);

    let r2 = Rect::new(20.0, 5.0, 10.0, 10.0);
    assert_eq!(
        union_rect(&Rect::new(0.0, 0.0, 10.0, 10.0), &r2),
        Rect::new(0.0, 0.0, 30.0, 15.0)
    );
}

#[test]
fn test_scene_bounds_and_hit_test() {
    let mut scene = FigureScene::new(400.0, 400.0);
    scene.append_node(rect_at(0.0, 0.0, 100.0, 50.0));
    let mut r2 = rect_at(200.0, 200.0, 100.0, 50.0);
    if let FigureNode::Rect(ref mut r) = r2 {
        r.id = "r2".to_string();
    }
    scene.append_node(r2);

    assert_eq!(
        scene_bounds(&scene),
        Rect {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 250.0,
        }
    );

    // Overlapping hit test: later node wins
    let mut stacked = FigureScene::new(400.0, 400.0);
    stacked.append_node(rect_at(0.0, 0.0, 200.0, 200.0));
    let mut s2 = rect_at(50.0, 50.0, 200.0, 200.0);
    if let FigureNode::Rect(ref mut r) = s2 {
        r.id = "r2".to_string();
    }
    stacked.append_node(s2);

    assert_eq!(
        hit_test(&stacked, Vec2::new(100.0, 100.0)).map(|n| n.id()),
        Some("r2")
    );
    assert!(hit_test(&scene, Vec2::new(390.0, 390.0)).is_none());
}

#[test]
fn test_anchors_and_snapping() {
    let rect = rect_at(100.0, 100.0, 200.0, 100.0);
    let anchors = anchors_of(&rect);

    assert_eq!(anchors.len(), 4);
    assert_eq!(anchors[0].side, AnchorSide::Top);
    assert_eq!(anchors[0].x, 200.0);
    assert_eq!(anchors[0].y, 100.0);

    assert_eq!(anchors[1].side, AnchorSide::Right);
    assert_eq!(anchors[1].x, 300.0);
    assert_eq!(anchors[1].y, 150.0);

    assert_eq!(anchors[2].side, AnchorSide::Bottom);
    assert_eq!(anchors[2].x, 200.0);
    assert_eq!(anchors[2].y, 200.0);

    assert_eq!(anchors[3].side, AnchorSide::Left);
    assert_eq!(anchors[3].x, 100.0);
    assert_eq!(anchors[3].y, 150.0);

    // Snap nearest
    let res = snap_to_anchors(Vec2::new(202.0, 104.0), &anchors, SNAP_DISTANCE);
    assert_eq!(res.point, Vec2::new(200.0, 100.0));
    assert_eq!(res.anchor.as_ref().map(|a| a.side), Some(AnchorSide::Top));

    // Beyond threshold
    let far = Vec2::new(200.0 + SNAP_DISTANCE + 1.0, 100.0);
    let far_res = snap_to_anchors(far, &anchors, SNAP_DISTANCE);
    assert_eq!(far_res.point, far);
    assert!(far_res.anchor.is_none());

    // all_anchors with exclusion
    let mut scene = FigureScene::new(400.0, 400.0);
    scene.append_node(rect.clone());
    let mut r2 = rect;
    if let FigureNode::Rect(ref mut r) = r2 {
        r.id = "r2".to_string();
        r.x = 300.0;
    }
    scene.append_node(r2);

    let filtered_anchors = manang_figure::all_anchors(&scene, &["r1"]);
    assert!(filtered_anchors.iter().all(|a| a.node_id == "r2"));
}

#[test]
fn test_stroke_to_path() {
    let empty = StrokeNode {
        id: "s".to_string(),
        points: vec![],
        size: 2.0,
        color: "#fff".to_string(),
    };
    assert_eq!(stroke_to_path(&empty), "");

    let tap = StrokeNode {
        id: "s".to_string(),
        points: vec![StrokeSample::new(1.0, 2.0, 0.5)],
        size: 2.0,
        color: "#fff".to_string(),
    };
    let path = stroke_to_path(&tap);
    assert!(path.starts_with('M'));
    assert!(path.ends_with('Z'));

    let points: Vec<StrokeSample> = (0..24)
        .map(|i| StrokeSample::new(i as f64 * 4.0, 40.0 + ((i as f64) / 2.0).sin() * 12.0, 0.5))
        .collect();

    let stroke = StrokeNode {
        id: "s".to_string(),
        points,
        size: 3.0,
        color: "#e8e6e1".to_string(),
    };
    let path = stroke_to_path(&stroke);
    assert!(path.starts_with('M'));
    assert!(path.ends_with('Z'));
    assert!(path.matches('L').count() > 10);
}

#[test]
fn test_arrow_head_and_polygon() {
    let head = arrow_head(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), 7.0);
    assert_eq!(head.len(), 4);
    assert_eq!(head[0], Vec2::new(100.0, 0.0));

    let poly_path = polygon_to_path(&[
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(10.0, 10.0),
    ]);
    assert_eq!(poly_path, "M0 0L10 0L10 10Z");
}

#[test]
fn test_node_id_formatting() {
    assert_eq!(format_node_id(1), "n1");
    assert_eq!(format_node_id(35), "nz");
    assert_eq!(format_node_id(36), "n10");
}
