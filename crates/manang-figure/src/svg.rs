//! Deterministic SVG export (ADR 0012, "Rendering").
//!
//! A pure function of the scene and the options: no clock, no randomness
//! (hand-drawn strokes are seeded by node id), no I/O. Mermaid blocks arrive
//! pre-rendered through `embeds`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::geometry::{format_coord, freedraw_path, node_bounds, node_rect, union_rect};
use crate::palette::{self, Theme};
use crate::route::end_directions;
use crate::scene::{
    ConnectorNode, FigureNode, FigureScene, FontFamily, Head, MathNode, Rect, ShapeNode,
    StrokeStyle, Style, TextAlign, Vec2,
};
use crate::sketch::{self, Rng};
use crate::text::{self, LINE_HEIGHT};

/// Options for SVG export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SvgExportOptions {
    /// Token values to draw with. Files use `print`.
    pub theme: Theme,
    /// Paint the `paper` colour behind everything.
    pub background: bool,
    /// Export only this frame's region.
    pub frame: Option<String>,
    /// Draw a 1px hairline around the page (v1 exports did).
    pub include_frame: bool,
}

impl Default for SvgExportOptions {
    fn default() -> Self {
        Self {
            theme: Theme::Print,
            background: false,
            frame: None,
            include_frame: false,
        }
    }
}

/// Escapes XML special characters.
pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// The region an export covers: a frame; else the page when everything
/// fits on it; else the content's bounds with a 16-unit margin.
pub fn export_region(scene: &FigureScene, frame: Option<&str>) -> Rect {
    if let Some(id) = frame
        && let Some(node) = scene.node_by_id(id)
    {
        return node_bounds(node);
    }
    let page = Rect::new(0.0, 0.0, scene.width, scene.height);
    let content = scene
        .nodes
        .iter()
        .filter(|n| !n.hidden())
        .fold(Rect::EMPTY, |acc, n| union_rect(&acc, &node_bounds(n)));
    if content.width == 0.0 && content.height == 0.0 || page.contains_rect(&content) {
        return page;
    }
    Rect::new(
        content.x - 16.0,
        content.y - 16.0,
        content.width + 32.0,
        content.height + 32.0,
    )
}

/// The backdrop of an export: an exported frame's own fill; else the
/// scene's `background`; else `paper` when `options.background` asks for it;
/// else none (transparent).
fn background_paint(scene: &FigureScene, options: &SvgExportOptions) -> Option<(String, f64)> {
    let theme = options.theme;
    if let Some(FigureNode::Frame(frame)) =
        options.frame.as_deref().and_then(|id| scene.node_by_id(id))
        && frame.style.fill() != "none"
    {
        let paint = palette::fill(frame.style.fill(), theme);
        let alpha = frame
            .style
            .fill_opacity
            .map_or(paint.alpha, |p| (p / 100.0).clamp(0.0, 1.0));
        return Some((paint.colour, alpha));
    }
    if let Some(bg) = &scene.background {
        return Some((palette::stroke(bg, theme), 1.0));
    }
    options
        .background
        .then(|| (palette::stroke("paper", theme), 1.0))
}

/// Renders with default options and no embeds.
pub fn render_svg(scene: &FigureScene, options: &SvgExportOptions) -> String {
    render_svg_with(scene, options, &BTreeMap::new())
}

/// Renders `scene`. `embeds` maps Mermaid block ids to their rendered SVG.
pub fn render_svg_with(
    scene: &FigureScene,
    options: &SvgExportOptions,
    embeds: &BTreeMap<String, String>,
) -> String {
    let region = export_region(scene, options.frame.as_deref());
    let (x, y, w, h) = (
        format_coord(region.x),
        format_coord(region.y),
        format_coord(region.width),
        format_coord(region.height),
    );
    let mut svg = String::with_capacity(256 + scene.nodes.len() * 160);
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{x} {y} {w} {h}\" width=\"{w}\" height=\"{h}\">"
    );
    let theme = options.theme;
    if let Some((colour, alpha)) = background_paint(scene, options) {
        let opacity = if alpha < 1.0 {
            format!(" fill-opacity=\"{}\"", format_coord(alpha))
        } else {
            String::new()
        };
        let _ = writeln!(
            svg,
            "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{colour}\"{opacity}/>"
        );
    }
    if options.include_frame && scene.width > 1.0 && scene.height > 1.0 {
        let _ = writeln!(
            svg,
            "  <rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1\"/>",
            format_coord(scene.width - 1.0),
            format_coord(scene.height - 1.0),
            palette::stroke("line", theme)
        );
    }
    let ctx = Ctx {
        scene,
        theme,
        halo: palette::stroke("paper", theme),
        export_frame: options.frame.as_deref(),
        embeds,
    };
    for node in scene.nodes.iter().filter(|n| !n.hidden()) {
        ctx.node(&mut svg, node);
    }
    svg.push_str("</svg>\n");
    svg
}

/// One block of text lines: where, in what font, in what colour.
#[derive(Clone, Copy)]
struct TextBlock<'a> {
    lines: &'a [String],
    x: f64,
    top: f64,
    size: f64,
    family: FontFamily,
    anchor: &'a str,
    colour: &'a str,
    halo: bool,
}

struct Ctx<'a> {
    scene: &'a FigureScene,
    theme: Theme,
    halo: String,
    export_frame: Option<&'a str>,
    embeds: &'a BTreeMap<String, String>,
}

fn dash(style: &Style) -> String {
    let w = style.stroke_width();
    match style.stroke_style() {
        StrokeStyle::Solid => String::new(),
        StrokeStyle::Dashed => format!(
            " stroke-dasharray=\"{} {}\"",
            format_coord(w * 4.0 + 2.0),
            format_coord(w * 3.0 + 2.0)
        ),
        StrokeStyle::Dotted => format!(
            " stroke-dasharray=\"{} {}\"",
            format_coord(w * 0.5),
            format_coord(w * 2.5 + 1.0)
        ),
    }
}

fn font(family: FontFamily) -> &'static str {
    match family {
        FontFamily::Sans => "DM Sans, sans-serif",
        FontFamily::Mono => "DM Mono, monospace",
    }
}

/// Default font size of connector labels.
pub const CONNECTOR_FONT_SIZE: f64 = 14.0;

/// The share of a diamond's width and height its text box (the inscribed
/// rectangle) uses.
pub const DIAMOND_TEXT_BOX: f64 = 0.6;

/// Horizontal padding inside a shape before its label.
pub const LABEL_PADDING: f64 = 8.0;

/// The width a label may use inside a box of `width` for `kind`.
pub fn label_width(kind: &str, width: f64) -> f64 {
    let usable = match kind {
        "diamond" => return (width.abs() * DIAMOND_TEXT_BOX).max(1.0),
        "ellipse" => width * 0.72,
        "text" => return width.abs(),
        _ => width.abs(),
    };
    (usable - 2.0 * LABEL_PADDING).max(1.0)
}

impl Ctx<'_> {
    fn stroke(&self, style: &Style) -> String {
        palette::stroke(style.stroke(), self.theme)
    }

    fn fill_attrs(&self, style: &Style) -> String {
        let mut paint = palette::fill(style.fill(), self.theme);
        if let Some(percent) = style.fill_opacity {
            paint.alpha = (percent / 100.0).clamp(0.0, 1.0);
        }
        if paint.alpha < 1.0 {
            format!(
                "fill=\"{}\" fill-opacity=\"{}\"",
                paint.colour,
                format_coord(paint.alpha)
            )
        } else {
            format!("fill=\"{}\"", paint.colour)
        }
    }

    fn open_group(&self, svg: &mut String, id: &str, style: &Style, rotate: Option<(f64, Vec2)>) {
        let _ = write!(svg, "  <g id=\"{}\"", escape_xml(id));
        if let Some((angle, c)) = rotate
            && angle != 0.0
        {
            let _ = write!(
                svg,
                " transform=\"rotate({} {} {})\"",
                format_coord(angle),
                format_coord(c.x),
                format_coord(c.y)
            );
        }
        let opacity = style.opacity();
        if opacity < 1.0 {
            let _ = write!(svg, " opacity=\"{}\"", format_coord(opacity));
        }
        svg.push_str(">\n");
    }

    fn node(&self, svg: &mut String, node: &FigureNode) {
        match node {
            FigureNode::Rect(s) | FigureNode::Diamond(s) | FigureNode::Ellipse(s) => {
                self.shape(svg, node.kind(), s);
            }
            FigureNode::Frame(s) => self.frame(svg, s),
            FigureNode::Text(s) => {
                self.open_group(svg, &s.id, &s.style, Some((s.angle, s.rect().center())));
                if let Some(label) = &s.label {
                    self.label(svg, "text", s, label);
                }
                svg.push_str("  </g>\n");
            }
            FigureNode::Image(s) => {
                self.open_group(svg, &s.id, &s.style, Some((s.angle, s.rect().center())));
                let r = node_rect(node);
                let _ = writeln!(
                    svg,
                    "    <image href=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"xMidYMid meet\"/>",
                    escape_xml(s.src.as_deref().unwrap_or_default()),
                    format_coord(r.x),
                    format_coord(r.y),
                    format_coord(r.width),
                    format_coord(r.height)
                );
                svg.push_str("  </g>\n");
            }
            FigureNode::Mermaid(s) => self.mermaid(svg, s),
            FigureNode::Arrow(c) => self.connector(svg, c, true),
            FigureNode::Line(c) => self.connector(svg, c, false),
            FigureNode::Freedraw(f) => {
                let d = freedraw_path(f);
                if d.is_empty() {
                    return;
                }
                self.open_group(svg, &f.id, &f.style, None);
                let _ = writeln!(
                    svg,
                    "    <path d=\"{d}\" fill=\"{}\"/>",
                    self.stroke(&f.style)
                );
                svg.push_str("  </g>\n");
            }
            FigureNode::Math(m) => self.math(svg, m),
        }
    }

    /// A formula: its rendered vector paths nested at the node's box, in
    /// the node's colour. Without a rendering (no engine, or a formula that
    /// does not compile) a dashed placeholder marks the place.
    fn math(&self, svg: &mut String, m: &MathNode) {
        self.open_group(svg, &m.id, &m.style, None);
        let colour = self.stroke(&m.style);
        let rendered = self.embeds.get(&m.id).and_then(|e| {
            let (w, h) = crate::math::svg_size(e)?;
            Some((nested_math(e, m, w, h, &colour)?, w, h))
        });
        match rendered {
            Some((nested, _, _)) => {
                svg.push_str("    ");
                svg.push_str(&nested);
                svg.push('\n');
            }
            None => {
                let (w, h) = m.box_size();
                let muted = palette::stroke("muted", self.theme);
                let _ = writeln!(
                    svg,
                    "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{muted}\" stroke-width=\"1\" stroke-dasharray=\"4 4\"/>",
                    format_coord(m.x),
                    format_coord(m.y),
                    format_coord(w),
                    format_coord(h)
                );
            }
        }
        svg.push_str("  </g>\n");
    }

    fn shape(&self, svg: &mut String, kind: &str, s: &ShapeNode) {
        let r = crate::geometry::normalize_rect(s.x, s.y, s.width, s.height);
        self.open_group(svg, &s.id, &s.style, Some((s.angle, r.center())));
        let roughness = s.style.roughness();
        let mut rng = Rng::for_id(&s.id);
        let clean = match kind {
            "diamond" => sketch::polygon(&sketch::diamond_points(&r), 0, &mut rng),
            "ellipse" => sketch::ellipse(&r, 0, &mut rng),
            _ => sketch::polygon(&sketch::rect_points(&r), 0, &mut rng),
        };
        let stroke = self.stroke(&s.style);
        let width = format_coord(s.style.stroke_width());
        let mut dash = dash(&s.style);
        if s.style.stroke_style() == StrokeStyle::Dotted && roughness == 0 {
            dash.push_str(" stroke-linecap=\"round\"");
        }
        if roughness == 0 {
            let _ = writeln!(
                svg,
                "    <path d=\"{clean}\" {} stroke=\"{stroke}\" stroke-width=\"{width}\"{dash}/>",
                self.fill_attrs(&s.style)
            );
        } else {
            if s.style.fill() != "none" {
                let _ = writeln!(
                    svg,
                    "    <path d=\"{clean}\" {}/>",
                    self.fill_attrs(&s.style)
                );
            }
            let rough = match kind {
                "diamond" => sketch::polygon(&sketch::diamond_points(&r), roughness, &mut rng),
                "ellipse" => sketch::ellipse(&r, roughness, &mut rng),
                _ => sketch::polygon(&sketch::rect_points(&r), roughness, &mut rng),
            };
            let _ = writeln!(
                svg,
                "    <path d=\"{rough}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{width}\" stroke-linecap=\"round\"{dash}/>"
            );
        }
        if let Some(label) = &s.label {
            self.label(svg, kind, s, label);
        }
        svg.push_str("  </g>\n");
    }

    fn frame(&self, svg: &mut String, s: &ShapeNode) {
        if self.export_frame == Some(s.id.as_str()) {
            return;
        }
        let r = crate::geometry::normalize_rect(s.x, s.y, s.width, s.height);
        self.open_group(svg, &s.id, &s.style, Some((s.angle, r.center())));
        let stroke = palette::stroke(s.style.stroke.as_deref().unwrap_or("muted"), self.theme);
        let _ = writeln!(
            svg,
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {} stroke=\"{stroke}\" stroke-width=\"1\"/>",
            format_coord(r.x),
            format_coord(r.y),
            format_coord(r.width),
            format_coord(r.height),
            self.fill_attrs(&s.style)
        );
        if let Some(label) = &s.label {
            let title = s
                .style
                .label_color
                .as_deref()
                .map_or_else(|| stroke.clone(), |c| palette::stroke(c, self.theme));
            let _ = writeln!(
                svg,
                "    <text x=\"{}\" y=\"{}\" fill=\"{title}\" font-family=\"{}\" font-size=\"12\">{}</text>",
                format_coord(r.x),
                format_coord(r.y - 6.0),
                font(FontFamily::Mono),
                escape_xml(label)
            );
        }
        svg.push_str("  </g>\n");
    }

    fn mermaid(&self, svg: &mut String, s: &ShapeNode) {
        let r = crate::geometry::normalize_rect(s.x, s.y, s.width, s.height);
        self.open_group(svg, &s.id, &s.style, Some((s.angle, r.center())));
        match self.embeds.get(&s.id).and_then(|e| nested_svg(e, &r)) {
            Some(nested) => {
                svg.push_str("    ");
                svg.push_str(&nested);
                svg.push('\n');
            }
            None => {
                let muted = palette::stroke("muted", self.theme);
                let _ = writeln!(
                    svg,
                    "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{muted}\" stroke-width=\"1\" stroke-dasharray=\"4 4\"/>",
                    format_coord(r.x),
                    format_coord(r.y),
                    format_coord(r.width),
                    format_coord(r.height)
                );
                let _ = writeln!(
                    svg,
                    "    <text x=\"{}\" y=\"{}\" fill=\"{muted}\" font-family=\"{}\" font-size=\"12\" text-anchor=\"middle\">Mermaid</text>",
                    format_coord(r.center().x),
                    format_coord(r.center().y + 4.0),
                    font(FontFamily::Mono)
                );
            }
        }
        svg.push_str("  </g>\n");
    }

    /// Wrapped label lines inside a box, vertically centred (top-aligned for
    /// text nodes).
    fn label(&self, svg: &mut String, kind: &str, s: &ShapeNode, label: &str) {
        let r = crate::geometry::normalize_rect(s.x, s.y, s.width, s.height);
        let style = &s.style;
        let family = style.font_family();
        let (lines, size) = if kind == "diamond" {
            text::fit_label(
                label,
                label_width(kind, r.width),
                r.height * DIAMOND_TEXT_BOX,
                style.font_size(),
                family,
            )
        } else {
            let size = style.font_size();
            (
                text::wrap(label, label_width(kind, r.width), size, family),
                size,
            )
        };
        let lh = size * LINE_HEIGHT;
        let total = lh * lines.len() as f64;
        let top = if kind == "text" {
            r.y
        } else {
            r.center().y - total / 2.0
        };
        let align = style.text_align.unwrap_or(if kind == "text" {
            TextAlign::Left
        } else {
            TextAlign::Center
        });
        let pad = if kind == "text" { 0.0 } else { LABEL_PADDING };
        let (x, anchor) = match align {
            TextAlign::Left => (r.x + pad, "start"),
            TextAlign::Center => (r.center().x, "middle"),
            TextAlign::Right => (r.right() - pad, "end"),
        };
        let colour = palette::stroke(style.label_colour(), self.theme);
        self.text_lines(
            svg,
            &TextBlock {
                lines: &lines,
                x,
                top,
                size,
                family,
                anchor,
                colour: &colour,
                halo: false,
            },
        );
    }

    fn text_lines(&self, svg: &mut String, block: &TextBlock<'_>) {
        let TextBlock {
            lines,
            x,
            top,
            size,
            family,
            anchor,
            colour,
            halo,
        } = *block;
        let lh = size * LINE_HEIGHT;
        let halo_attrs = if halo {
            format!(
                " stroke=\"{}\" stroke-width=\"4\" stroke-linejoin=\"round\" paint-order=\"stroke\"",
                self.halo
            )
        } else {
            String::new()
        };
        let _ = write!(
            svg,
            "    <text fill=\"{colour}\" font-family=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\"{halo_attrs}>",
            font(family),
            format_coord(size)
        );
        for (i, line) in lines.iter().enumerate() {
            let baseline = top + lh * i as f64 + lh / 2.0 + size * 0.35;
            let _ = write!(
                svg,
                "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                format_coord(x),
                format_coord(baseline),
                escape_xml(line)
            );
        }
        svg.push_str("</text>\n");
    }

    fn connector(&self, svg: &mut String, c: &ConnectorNode, arrow: bool) {
        if c.points.len() < 2 {
            return;
        }
        self.open_group(svg, &c.id, &c.style, None);
        let roughness = c.style.roughness();
        let mut rng = Rng::for_id(&c.id);
        let (ds, de) = end_directions(self.scene, c);
        let (d, start_dir, end_dir) =
            sketch::connector(&c.points, c.routing, ds, de, roughness, &mut rng);
        let stroke = self.stroke(&c.style);
        let width = c.style.stroke_width();
        let _ = writeln!(
            svg,
            "    <path d=\"{d}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{}/>",
            format_coord(width),
            dash(&c.style)
        );
        let n = c.points.len();
        let start_head = c.heads.start.unwrap_or(Head::None);
        let end_head = c
            .heads
            .end
            .unwrap_or(if arrow { Head::Arrow } else { Head::None });
        for (head, tip, dir) in [
            (start_head, c.points[0], start_dir),
            (end_head, c.points[n - 1], end_dir),
        ] {
            if let Some(path) = head_svg(head, tip, dir, width, &stroke) {
                svg.push_str("    ");
                svg.push_str(&path);
                svg.push('\n');
            }
        }
        if let Some(label) = &c.label {
            let mid = path_midpoint(&c.points);
            let size = c.style.font_size.unwrap_or(CONNECTOR_FONT_SIZE);
            let family = c.style.font_family();
            let lines = text::wrap(label, 0.0, size, family);
            let total = size * LINE_HEIGHT * lines.len() as f64;
            self.text_lines(
                svg,
                &TextBlock {
                    lines: &lines,
                    x: mid.x,
                    top: mid.y - total / 2.0,
                    size,
                    family,
                    anchor: "middle",
                    colour: &palette::stroke(c.style.label_colour(), self.theme),
                    halo: true,
                },
            );
        }
        svg.push_str("  </g>\n");
    }
}

/// The point halfway along a polyline.
pub fn path_midpoint(points: &[Vec2]) -> Vec2 {
    let total: f64 = points
        .windows(2)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .sum();
    let mut left = total / 2.0;
    for w in points.windows(2) {
        let len = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        if len >= left && len > 0.0 {
            let t = left / len;
            return Vec2::new(
                w[0].x + (w[1].x - w[0].x) * t,
                w[0].y + (w[1].y - w[0].y) * t,
            );
        }
        left -= len;
    }
    points.first().copied().unwrap_or(Vec2::new(0.0, 0.0))
}

/// The SVG of one arrow head at `tip`, pointing along `dir`.
pub fn head_svg(head: Head, tip: Vec2, dir: Vec2, width: f64, colour: &str) -> Option<String> {
    let len = dir.x.hypot(dir.y);
    if head == Head::None || len < 1e-9 {
        return None;
    }
    let (ux, uy) = (dir.x / len, dir.y / len);
    let size = 7.0 + width * 2.0;
    let back = |k: f64, spread: f64| {
        let (s, c) = spread.sin_cos();
        // Rotate (-u) by ±spread.
        let (bx, by) = (-ux, -uy);
        Vec2::new(
            tip.x + size * k * (bx * c - by * s),
            tip.y + size * k * (bx * s + by * c),
        )
    };
    let spread = std::f64::consts::PI / 7.0;
    let w = format_coord(width);
    Some(match head {
        Head::Arrow => {
            let (a, b) = (back(1.0, spread), back(1.0, -spread));
            format!(
                "<path d=\"M{} {}L{} {}L{} {}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"{w}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                format_coord(a.x),
                format_coord(a.y),
                format_coord(tip.x),
                format_coord(tip.y),
                format_coord(b.x),
                format_coord(b.y)
            )
        }
        Head::Triangle => {
            let (a, b) = (back(1.0, spread), back(1.0, -spread));
            format!(
                "<path d=\"{}\" fill=\"{colour}\" stroke=\"{colour}\" stroke-width=\"{w}\" stroke-linejoin=\"round\"/>",
                crate::geometry::polygon_to_path(&[tip, a, b])
            )
        }
        Head::Dot => {
            let r = size / 3.0;
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{colour}\"/>",
                format_coord(tip.x - ux * r),
                format_coord(tip.y - uy * r),
                format_coord(r)
            )
        }
        Head::Bar => {
            let h = size / 2.0;
            format!(
                "<path d=\"M{} {}L{} {}\" stroke=\"{colour}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>",
                format_coord(tip.x - uy * h),
                format_coord(tip.y + ux * h),
                format_coord(tip.x + uy * h),
                format_coord(tip.y - ux * h)
            )
        }
        Head::None => return None,
    })
}

/// A rendered formula as a nested `<svg>` at the node's corner, painting in
/// `colour` (the rendering's own paints are `currentColor`).
fn nested_math(document: &str, m: &MathNode, w: f64, h: f64, colour: &str) -> Option<String> {
    let start = document.find("<svg")?;
    let open_end = start + document[start..].find('>')?;
    let close = document.rfind("</svg>")?;
    if close < open_end {
        return None;
    }
    let inner = document[open_end + 1..close].trim();
    Some(format!(
        "<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" color=\"{colour}\" fill=\"{colour}\">{inner}</svg>",
        format_coord(m.x),
        format_coord(m.y),
        format_coord(w),
        format_coord(h),
        format_coord(w),
        format_coord(h),
    ))
}

/// A rendered SVG document as a nested `<svg>` filling `r`: the prolog is
/// dropped, the root's `viewBox` kept, its size replaced.
fn nested_svg(document: &str, r: &Rect) -> Option<String> {
    let start = document.find("<svg")?;
    let open_end = start + document[start..].find('>')?;
    let close = document.rfind("</svg>")?;
    if close < open_end {
        return None;
    }
    let root = &document[start..open_end];
    let view_box = root
        .find("viewBox=\"")
        .and_then(|i| {
            let rest = &root[i + 9..];
            rest.find('"').map(|j| &rest[..j])
        })
        .unwrap_or("0 0 100 100");
    // The renderer scopes its `<style>` rules to the root's id (`#mng-… .node
    // rect`); without the id on the nested element they match nothing and
    // every shape falls back to a black fill.
    let id = root
        .find(" id=\"")
        .and_then(|i| {
            let rest = &root[i + 5..];
            rest.find('"').map(|j| &rest[..j])
        })
        .map(|id| format!(" id=\"{id}\""))
        .unwrap_or_default();
    let inner = document[open_end + 1..close].trim();
    Some(format!(
        "<svg{id} x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"{}\" preserveAspectRatio=\"xMidYMid meet\">{inner}</svg>",
        format_coord(r.x),
        format_coord(r.y),
        format_coord(r.width),
        format_coord(r.height),
        escape_xml(view_box)
    ))
}

impl FigureScene {
    /// The scene as SVG with default options (print theme).
    pub fn to_svg(&self) -> String {
        render_svg(self, &SvgExportOptions::default())
    }
}
