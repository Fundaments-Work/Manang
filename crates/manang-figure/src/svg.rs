use serde::{Deserialize, Serialize};

use crate::geometry::{arrow_head, format_coord, polygon_to_path, stroke_to_path};
use crate::scene::{FigureNode, FigureScene};

/// Options for headless SVG export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SvgExportOptions {
    /// Whether to render the 1px hairline boundary frame.
    pub include_frame: bool,
    /// Color of the perimeter frame line. Default is "#3d3d42" (dark hairline token).
    pub frame_stroke: String,
}

impl Default for SvgExportOptions {
    fn default() -> Self {
        Self {
            include_frame: true,
            frame_stroke: "#3d3d42".to_string(),
        }
    }
}

/// Escapes XML special characters for safe inclusion in SVG attribute and text nodes.
pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Renders a `FigureScene` into a deterministic, headless SVG string.
pub fn render_svg(scene: &FigureScene, options: &SvgExportOptions) -> String {
    let mut svg = String::new();
    let width_str = format_coord(scene.width);
    let height_str = format_coord(scene.height);

    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width_str} {height_str}\" width=\"{width_str}\" height=\"{height_str}\">\n"
    ));

    if options.include_frame && scene.width > 1.0 && scene.height > 1.0 {
        let frame_w = format_coord(scene.width - 1.0);
        let frame_h = format_coord(scene.height - 1.0);
        svg.push_str(&format!(
            "  <rect x=\"0.5\" y=\"0.5\" width=\"{frame_w}\" height=\"{frame_h}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1\"/>\n",
            options.frame_stroke
        ));
    }

    for node in &scene.nodes {
        match node {
            FigureNode::Stroke(s) => {
                let path = stroke_to_path(s);
                if !path.is_empty() {
                    svg.push_str(&format!(
                        "  <path id=\"{}\" d=\"{path}\" fill=\"{}\"/>\n",
                        escape_xml(&s.id),
                        escape_xml(&s.color)
                    ));
                }
            }
            FigureNode::Rect(r) => {
                let fill = r.fill.as_deref().unwrap_or("none");
                svg.push_str(&format!(
                    "  <rect id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
                    escape_xml(&r.id),
                    format_coord(r.x),
                    format_coord(r.y),
                    format_coord(r.width),
                    format_coord(r.height),
                    escape_xml(fill),
                    escape_xml(&r.stroke)
                ));
            }
            FigureNode::Ellipse(e) => {
                let fill = e.fill.as_deref().unwrap_or("none");
                svg.push_str(&format!(
                    "  <ellipse id=\"{}\" cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
                    escape_xml(&e.id),
                    format_coord(e.cx),
                    format_coord(e.cy),
                    format_coord(e.rx),
                    format_coord(e.ry),
                    escape_xml(fill),
                    escape_xml(&e.stroke)
                ));
            }
            FigureNode::Arrow(a) => {
                svg.push_str(&format!("  <g id=\"{}\">\n", escape_xml(&a.id)));
                svg.push_str(&format!(
                    "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
                    format_coord(a.from.x),
                    format_coord(a.from.y),
                    format_coord(a.to.x),
                    format_coord(a.to.y),
                    escape_xml(&a.color)
                ));
                let head = arrow_head(a.from, a.to, 7.0);
                let head_path = polygon_to_path(&head);
                svg.push_str(&format!(
                    "    <path d=\"{head_path}\" fill=\"{}\"/>\n",
                    escape_xml(&a.color)
                ));
                if let Some(ref label) = a.label {
                    let mid_x = format_coord((a.from.x + a.to.x) / 2.0);
                    let mid_y = format_coord((a.from.y + a.to.y) / 2.0 - 8.0);
                    svg.push_str(&format!(
                        "    <text x=\"{mid_x}\" y=\"{mid_y}\" fill=\"#9a9a9a\" font-family=\"DM Mono, monospace\" font-size=\"13\" text-anchor=\"middle\">{}</text>\n",
                        escape_xml(label)
                    ));
                }
                svg.push_str("  </g>\n");
            }
            FigureNode::Math(m) => {
                svg.push_str(&format!(
                    "  <text id=\"{}\" x=\"{}\" y=\"{}\" fill=\"{}\" font-family=\"DM Mono, monospace\" font-size=\"{}\">{}</text>\n",
                    escape_xml(&m.id),
                    format_coord(m.x),
                    format_coord(m.y),
                    escape_xml(&m.color),
                    format_coord(m.size),
                    escape_xml(&m.tex)
                ));
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

impl FigureScene {
    /// Deterministically renders this scene to headless SVG with default options.
    pub fn to_svg(&self) -> String {
        render_svg(self, &SvgExportOptions::default())
    }
}
