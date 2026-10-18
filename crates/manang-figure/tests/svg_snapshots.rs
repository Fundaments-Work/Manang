//! SVG export snapshots: every shape kind in the clean and hand-drawn
//! styles, heads, routing, labels, themes and frame export. Any change to
//! the exported bytes shows up here for review.

use std::collections::BTreeMap;

use manang_figure::{
    Binding, ConnectorNode, FigureNode, FigureScene, FreedrawNode, Head, Heads, MathNode, Routing,
    ShapeNode, StrokeSample, StrokeStyle, Style, SvgExportOptions, Theme, Vec2, render_svg,
    render_svg_with,
};

fn shape(kind: &str, id: &str, x: f64, y: f64, label: Option<&str>, style: Style) -> FigureNode {
    let mut s = ShapeNode::new(id, x, y, 140.0, 70.0);
    s.label = label.map(str::to_owned);
    s.style = style;
    match kind {
        "rect" => FigureNode::Rect(s),
        "diamond" => FigureNode::Diamond(s),
        "ellipse" => FigureNode::Ellipse(s),
        "text" => FigureNode::Text(s),
        "frame" => FigureNode::Frame(s),
        "image" => {
            s.src = Some("figures/photo.png".into());
            FigureNode::Image(s)
        }
        _ => {
            s.source = Some("flowchart LR\nA-->B".into());
            FigureNode::Mermaid(s)
        }
    }
}

fn every_kind(roughness: u8) -> FigureScene {
    let style = Style {
        roughness: Some(roughness),
        ..Style::default()
    };
    let filled = Style {
        fill: Some("blue".into()),
        stroke: Some("blue".into()),
        ..style.clone()
    };
    let mut scene = FigureScene::new(800.0, 400.0);
    scene.nodes = vec![
        shape(
            "rect",
            "r",
            20.0,
            20.0,
            Some("A box with a long label that wraps"),
            filled,
        ),
        shape("diamond", "d", 220.0, 20.0, Some("Valid?"), style.clone()),
        shape(
            "ellipse",
            "e",
            420.0,
            20.0,
            Some("Start"),
            Style {
                fill: Some("surface".into()),
                ..style.clone()
            },
        ),
        shape(
            "text",
            "t",
            620.0,
            20.0,
            Some("Plain text\nsecond line"),
            Style::default(),
        ),
        shape("image", "i", 20.0, 200.0, None, Style::default()),
        shape("mermaid", "m", 220.0, 200.0, None, Style::default()),
        FigureNode::Freedraw(FreedrawNode {
            id: "f".into(),
            points: vec![
                StrokeSample::new(450.0, 220.0, 0.5),
                StrokeSample::new(480.0, 250.0, 0.6),
                StrokeSample::new(520.0, 240.0, 0.5),
            ],
            size: 3.0,
            style: Style::default(),
            groups: Vec::new(),
            locked: false,
        }),
        FigureNode::Math(MathNode {
            id: "x".into(),
            x: 620.0,
            y: 240.0,
            tex: "E = mc^2".into(),
            size: 20.0,
            style: Style {
                stroke: Some("sage".into()),
                ..Style::default()
            },
            groups: Vec::new(),
            locked: false,
        }),
        {
            let mut c = ConnectorNode::new("a1", Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
            c.start = Some(Binding::to("r"));
            c.end = Some(Binding::to("d"));
            c.label = Some("yes".into());
            c.style = style.clone();
            FigureNode::Arrow(c)
        },
        {
            let mut c = ConnectorNode::new("a2", Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
            c.start = Some(Binding::to("d"));
            c.end = Some(Binding::to("e"));
            c.routing = Routing::Elbow;
            c.heads = Heads {
                start: Some(Head::Dot),
                end: Some(Head::Triangle),
            };
            c.style = Style {
                stroke_style: Some(StrokeStyle::Dashed),
                ..style.clone()
            };
            FigureNode::Arrow(c)
        },
        {
            let mut c = ConnectorNode::new("l1", Vec2::new(100.0, 350.0), Vec2::new(300.0, 300.0));
            c.routing = Routing::Curved;
            c.heads.end = Some(Head::Bar);
            c.style = Style {
                stroke_style: Some(StrokeStyle::Dotted),
                ..style
            };
            FigureNode::Line(c)
        },
    ];
    // Bound arrows get their points from routing, as `figure.apply` does.
    let routed: Vec<(String, Vec<Vec2>)> = scene
        .nodes
        .iter()
        .filter_map(FigureNode::connector)
        .map(|c| (c.id.clone(), manang_figure::route(&scene, c)))
        .collect();
    for (id, points) in routed {
        if let Some(c) = scene.node_mut(&id).and_then(FigureNode::connector_mut) {
            c.points = points;
        }
    }
    scene
}

#[test]
fn every_kind_clean() {
    insta::assert_snapshot!(render_svg(&every_kind(0), &SvgExportOptions::default()));
}

#[test]
fn every_kind_hand_drawn() {
    insta::assert_snapshot!(render_svg(&every_kind(1), &SvgExportOptions::default()));
}

#[test]
fn dark_theme_with_background() {
    let options = SvgExportOptions {
        theme: Theme::Dark,
        background: true,
        ..SvgExportOptions::default()
    };
    insta::assert_snapshot!(render_svg(&every_kind(0), &options));
}

#[test]
fn hand_drawn_output_is_stable() {
    let scene = every_kind(2);
    assert_eq!(
        render_svg(&scene, &SvgExportOptions::default()),
        render_svg(&scene, &SvgExportOptions::default())
    );
}

#[test]
fn a_frame_exports_its_region_and_a_mermaid_embed() {
    let mut scene = FigureScene::new(800.0, 400.0);
    let mut frame = ShapeNode::new("fr", 10.0, 10.0, 300.0, 200.0);
    frame.label = Some("Figure 1".into());
    scene.nodes.push(FigureNode::Frame(frame));
    scene.nodes.push(shape(
        "rect",
        "inside",
        40.0,
        40.0,
        Some("in"),
        Style::default(),
    ));
    scene
        .nodes
        .push(shape("mermaid", "m", 140.0, 100.0, None, Style::default()));
    let mut embeds = BTreeMap::new();
    embeds.insert(
        "m".to_owned(),
        "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 50 20\" width=\"50\"><rect width=\"50\" height=\"20\"/></svg>".to_owned(),
    );
    let options = SvgExportOptions {
        frame: Some("fr".into()),
        ..SvgExportOptions::default()
    };
    let svg = render_svg_with(&scene, &options, &embeds);
    assert!(svg.contains("viewBox=\"10 10 300 200\""));
    assert!(
        !svg.contains("Figure 1"),
        "the exported frame's own border is not drawn"
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn content_outside_the_page_grows_the_export() {
    let mut scene = FigureScene::new(100.0, 100.0);
    scene
        .nodes
        .push(shape("rect", "far", 300.0, 300.0, None, Style::default()));
    let svg = render_svg(&scene, &SvgExportOptions::default());
    assert!(svg.contains("viewBox=\"0 0 456 386\""), "{svg}");
}

/// The SVG is well-formed XML with no duplicate attributes.
fn assert_well_formed(svg: &str) {
    let mut reader = quick_xml::Reader::from_str(svg);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)) => {
                let mut seen = std::collections::HashSet::new();
                for attr in e.attributes() {
                    let attr = attr.expect("attribute parses");
                    assert!(
                        seen.insert(attr.key.as_ref().to_owned()),
                        "duplicate attribute in {svg}"
                    );
                }
            }
            Ok(_) => {}
            Err(e) => panic!("malformed SVG: {e}\n{svg}"),
        }
    }
}

#[test]
fn every_export_is_well_formed_xml() {
    for roughness in 0..3 {
        for theme in [Theme::Print, Theme::Light, Theme::Dark] {
            let options = SvgExportOptions {
                theme,
                background: roughness == 1,
                ..SvgExportOptions::default()
            };
            assert_well_formed(&render_svg(&every_kind(roughness), &options));
        }
    }
}
