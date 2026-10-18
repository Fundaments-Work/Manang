//! The figure scene graph, format version 2 (ADR 0012).
//!
//! A scene is plain data: a page size and an ordered list of nodes, painted
//! first to last. It serialises to the `.figure.json` files people commit,
//! so every optional field is omitted at its default and numbers are written
//! with at most two decimals ([`FigureScene::to_file_json`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::FigureError;

/// Version tag written by this crate.
pub const SCENE_VERSION: u32 = 2;

/// A 2D vector in scene coordinates (1 unit = 1 CSS pixel at 100% zoom).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vec2 {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl Vec2 {
    /// Creates a new `Vec2`.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in scene coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width (non-negative once normalised).
    pub width: f64,
    /// Height (non-negative once normalised).
    pub height: f64,
}

impl Rect {
    /// Creates a new `Rect`.
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// An empty rectangle at the origin.
    pub const EMPTY: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// The centre point.
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Right edge.
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge.
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Whether `p` lies inside or on the edge.
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// Whether `other` lies entirely inside this rectangle.
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    /// Whether the two rectangles overlap (touching counts).
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x <= other.right()
            && other.x <= self.right()
            && self.y <= other.bottom()
            && other.y <= self.bottom()
    }
}

/// A sampled point on a freehand path with pen pressure.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeSample {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Pressure (0.0 to 1.0).
    pub pressure: f64,
}

impl StrokeSample {
    /// Creates a new `StrokeSample`.
    pub const fn new(x: f64, y: f64, pressure: f64) -> Self {
        Self { x, y, pressure }
    }
}

/// How a stroke is dashed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeStyle {
    /// Continuous.
    #[default]
    Solid,
    /// Long dashes.
    Dashed,
    /// Dots.
    Dotted,
}

/// Typeface for labels and text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontFamily {
    /// DM Sans.
    #[default]
    Sans,
    /// DM Mono.
    Mono,
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    /// Flush left.
    Left,
    /// Centred.
    #[default]
    Center,
    /// Flush right.
    Right,
}

/// Per-node style. Every field is optional; `None` means the default, so
/// files stay small and a default change is one place.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    /// Stroke colour: a palette token or `#rrggbb`. Default `ink`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    /// Fill colour: `none`, a palette token or `#rrggbb`. Default `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// Stroke width in scene units. Default 1.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    /// Dashing. Default solid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_style: Option<StrokeStyle>,
    /// Opacity, 0 to 100. Default 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// 0 clean, 1 sketchy, 2 very sketchy. Default 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<u8>,
    /// Typeface. Default sans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<FontFamily>,
    /// Font size in scene units. Default 16.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Alignment. Default centre (left for text nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    /// Label and text colour: a palette token or `#rrggbb`. Default: the
    /// stroke colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<String>,
    /// Fill opacity, 0 to 100, on top of the overall opacity. Default: the
    /// palette tint for a hue token, 100 otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_opacity: Option<f64>,
}

/// Default stroke width.
pub const DEFAULT_STROKE_WIDTH: f64 = 1.5;
/// Default font size.
pub const DEFAULT_FONT_SIZE: f64 = 16.0;

impl Style {
    /// Whether every field is at its default.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The stroke colour.
    pub fn stroke(&self) -> &str {
        self.stroke.as_deref().unwrap_or("ink")
    }

    /// The label colour: `label_color`, else the stroke colour.
    pub fn label_colour(&self) -> &str {
        self.label_color.as_deref().unwrap_or_else(|| self.stroke())
    }

    /// The fill colour.
    pub fn fill(&self) -> &str {
        self.fill.as_deref().unwrap_or("none")
    }

    /// The stroke width.
    pub fn stroke_width(&self) -> f64 {
        self.stroke_width.unwrap_or(DEFAULT_STROKE_WIDTH)
    }

    /// The dash style.
    pub fn stroke_style(&self) -> StrokeStyle {
        self.stroke_style.unwrap_or_default()
    }

    /// Opacity as 0..1.
    pub fn opacity(&self) -> f64 {
        (self.opacity.unwrap_or(100.0) / 100.0).clamp(0.0, 1.0)
    }

    /// Roughness, clamped to 0..=2.
    pub fn roughness(&self) -> u8 {
        self.roughness.unwrap_or(0).min(2)
    }

    /// The typeface.
    pub fn font_family(&self) -> FontFamily {
        self.font_family.unwrap_or_default()
    }

    /// The font size.
    pub fn font_size(&self) -> f64 {
        self.font_size.unwrap_or(DEFAULT_FONT_SIZE)
    }

    /// Every colour field, as written.
    pub fn colours(&self) -> impl Iterator<Item = &str> {
        [&self.stroke, &self.fill, &self.label_color]
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    /// Rewrites hex literals as lowercase `#rrggbb`.
    pub fn normalise_colours(&mut self) {
        for c in [&mut self.stroke, &mut self.fill, &mut self.label_color]
            .into_iter()
            .flatten()
        {
            *c = crate::palette::normalize_colour(c);
        }
    }

    /// Merge `other` over `self`: every field `other` sets wins.
    pub fn merged(&self, other: &Style) -> Style {
        Style {
            stroke: other.stroke.clone().or_else(|| self.stroke.clone()),
            fill: other.fill.clone().or_else(|| self.fill.clone()),
            stroke_width: other.stroke_width.or(self.stroke_width),
            stroke_style: other.stroke_style.or(self.stroke_style),
            opacity: other.opacity.or(self.opacity),
            roughness: other.roughness.or(self.roughness),
            font_family: other.font_family.or(self.font_family),
            font_size: other.font_size.or(self.font_size),
            text_align: other.text_align.or(self.text_align),
            label_color: other
                .label_color
                .clone()
                .or_else(|| self.label_color.clone()),
            fill_opacity: other.fill_opacity.or(self.fill_opacity),
        }
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// A box-shaped node: rectangle, diamond, ellipse, frame, text, image or
/// Mermaid block. Kind-specific fields: `label` (shapes, frames, text
/// content), `src` (image), `source` (Mermaid).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeNode {
    /// Unique node id.
    pub id: String,
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// Rotation in degrees, clockwise, about the box centre.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: f64,
    /// The label drawn inside (for text nodes, the text itself; for frames,
    /// the title).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Image source: a path relative to the figure or a `data:` URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    /// Mermaid source of a Mermaid block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Style overrides.
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    /// Group ids, innermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// Locked nodes cannot be selected on the canvas.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

impl ShapeNode {
    /// A box with an id and nothing else set.
    pub fn new(id: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            id: id.into(),
            x,
            y,
            width,
            height,
            angle: 0.0,
            label: None,
            src: None,
            source: None,
            style: Style::default(),
            groups: Vec::new(),
            locked: false,
        }
    }

    /// The unrotated box.
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

/// Where an arrow end attaches to a shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// Id of the bound shape.
    pub node: String,
    /// Fixed point on the shape, as fractions of its box (0..1). Absent:
    /// the end floats along the outline toward the other end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Vec2>,
}

impl Binding {
    /// A floating binding to `node`.
    pub fn to(node: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            anchor: None,
        }
    }
}

/// How a connector runs between its ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Routing {
    /// Straight segments through the points.
    #[default]
    Straight,
    /// Horizontal and vertical segments only.
    Elbow,
    /// A smooth curve.
    Curved,
}

impl Routing {
    fn is_straight(&self) -> bool {
        *self == Routing::Straight
    }
}

/// An arrow head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Head {
    /// No head.
    None,
    /// Open chevron.
    Arrow,
    /// Filled triangle.
    Triangle,
    /// Filled dot.
    Dot,
    /// Perpendicular bar.
    Bar,
}

/// Heads at either end of a connector. `None` means the kind's default
/// (arrows: none at the start, `arrow` at the end; lines: none).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heads {
    /// Head at the first point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Head>,
    /// Head at the last point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Head>,
}

impl Heads {
    fn is_default(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }
}

/// An arrow or a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorNode {
    /// Unique node id.
    pub id: String,
    /// Absolute points, at least two.
    pub points: Vec<Vec2>,
    /// Binding of the first point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Binding>,
    /// Binding of the last point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Binding>,
    /// Routing.
    #[serde(default, skip_serializing_if = "Routing::is_straight")]
    pub routing: Routing,
    /// Heads.
    #[serde(default, skip_serializing_if = "Heads::is_default")]
    pub heads: Heads,
    /// Label at the middle of the path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Style overrides.
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    /// Group ids, innermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// Locked.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

impl ConnectorNode {
    /// An unbound straight connector from `from` to `to`.
    pub fn new(id: impl Into<String>, from: Vec2, to: Vec2) -> Self {
        Self {
            id: id.into(),
            points: vec![from, to],
            start: None,
            end: None,
            routing: Routing::Straight,
            heads: Heads::default(),
            label: None,
            style: Style::default(),
            groups: Vec::new(),
            locked: false,
        }
    }
}

/// A freehand pressure-tapered stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreedrawNode {
    /// Unique node id.
    pub id: String,
    /// Sampled points.
    pub points: Vec<StrokeSample>,
    /// Base diameter in scene units.
    pub size: f64,
    /// Style overrides (the stroke colour fills the outline).
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    /// Group ids, innermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// Locked.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

/// The language a formula is written in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MathLanguage {
    /// Typst math (`integral_0^1 x^2 dif x`), rendered by the Typst compiler.
    #[default]
    Typst,
    /// LaTeX math (`\int_0^1 x^2\,dx`), rendered by RaTeX.
    Latex,
}

impl MathLanguage {
    fn is_typst(&self) -> bool {
        *self == Self::Typst
    }

    /// The name as written in JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Typst => "typst",
            Self::Latex => "latex",
        }
    }
}

impl std::str::FromStr for MathLanguage {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "typst" => Ok(Self::Typst),
            "latex" => Ok(Self::Latex),
            other => Err(format!(
                "unknown math language `{other}`; use typst or latex"
            )),
        }
    }
}

/// A formula: source text in Typst or LaTeX math, rendered to vector paths
/// (never text) by [`crate::math`]. The box is measured from the rendering
/// and stored, so selection, alignment and arrow binding need no renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MathNode {
    /// Unique node id.
    pub id: String,
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Formula source (the inside of `$ .. $` in Typst, of `$$ .. $$` in LaTeX).
    pub source: String,
    /// Source language; Typst when absent.
    #[serde(default, skip_serializing_if = "MathLanguage::is_typst")]
    pub language: MathLanguage,
    /// Font size in scene units.
    pub size: f64,
    /// Measured width of the rendering (0: not measured yet, estimated).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: f64,
    /// Measured height of the rendering (0: not measured yet, estimated).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub height: f64,
    /// Style overrides (stroke colour = formula colour; default `ink`).
    #[serde(default, skip_serializing_if = "Style::is_empty")]
    pub style: Style,
    /// Group ids, innermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// Locked.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

impl MathNode {
    /// A formula with no measurement yet.
    pub fn new(
        id: impl Into<String>,
        x: f64,
        y: f64,
        source: impl Into<String>,
        language: MathLanguage,
        size: f64,
    ) -> Self {
        Self {
            id: id.into(),
            x,
            y,
            source: source.into(),
            language,
            size,
            width: 0.0,
            height: 0.0,
            style: Style::default(),
            groups: Vec::new(),
            locked: false,
        }
    }

    /// Width and height: the measured box, or an estimate from the source
    /// until the formula has been rendered once.
    pub fn box_size(&self) -> (f64, f64) {
        let width = if self.width > 0.0 {
            self.width
        } else {
            crate::text::measure(&self.source, self.size, FontFamily::Mono).max(self.size)
        };
        let height = if self.height > 0.0 {
            self.height
        } else {
            self.size * 1.4
        };
        (width, height)
    }
}

/// A scene node, tagged with `"kind"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FigureNode {
    /// Rectangle.
    Rect(ShapeNode),
    /// Diamond (rhombus inscribed in the box).
    Diamond(ShapeNode),
    /// Ellipse inscribed in the box.
    Ellipse(ShapeNode),
    /// Free text (the label is the text).
    Text(ShapeNode),
    /// A frame: a titled region and export area.
    Frame(ShapeNode),
    /// An image.
    Image(ShapeNode),
    /// A Mermaid block rendered by the diagram plugin.
    Mermaid(ShapeNode),
    /// Arrow (default head at the end).
    Arrow(ConnectorNode),
    /// Line (no heads by default).
    Line(ConnectorNode),
    /// Freehand stroke.
    Freedraw(FreedrawNode),
    /// Formula.
    Math(MathNode),
}

impl FigureNode {
    /// The kind tag as written in JSON.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Rect(_) => "rect",
            Self::Diamond(_) => "diamond",
            Self::Ellipse(_) => "ellipse",
            Self::Text(_) => "text",
            Self::Frame(_) => "frame",
            Self::Image(_) => "image",
            Self::Mermaid(_) => "mermaid",
            Self::Arrow(_) => "arrow",
            Self::Line(_) => "line",
            Self::Freedraw(_) => "freedraw",
            Self::Math(_) => "math",
        }
    }

    /// The node id.
    pub fn id(&self) -> &str {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => &s.id,
            Self::Arrow(c) | Self::Line(c) => &c.id,
            Self::Freedraw(f) => &f.id,
            Self::Math(m) => &m.id,
        }
    }

    /// The box-shaped part, for box kinds.
    pub fn shape(&self) -> Option<&ShapeNode> {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => Some(s),
            _ => None,
        }
    }

    /// Mutable box-shaped part.
    pub fn shape_mut(&mut self) -> Option<&mut ShapeNode> {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => Some(s),
            _ => None,
        }
    }

    /// The connector part, for arrows and lines.
    pub fn connector(&self) -> Option<&ConnectorNode> {
        match self {
            Self::Arrow(c) | Self::Line(c) => Some(c),
            _ => None,
        }
    }

    /// Mutable connector part.
    pub fn connector_mut(&mut self) -> Option<&mut ConnectorNode> {
        match self {
            Self::Arrow(c) | Self::Line(c) => Some(c),
            _ => None,
        }
    }

    /// Whether arrows may bind to this node.
    pub fn is_bindable(&self) -> bool {
        matches!(
            self,
            Self::Rect(_)
                | Self::Diamond(_)
                | Self::Ellipse(_)
                | Self::Text(_)
                | Self::Frame(_)
                | Self::Image(_)
                | Self::Mermaid(_)
                | Self::Math(_)
        )
    }

    /// The style.
    pub fn style(&self) -> &Style {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => &s.style,
            Self::Arrow(c) | Self::Line(c) => &c.style,
            Self::Freedraw(f) => &f.style,
            Self::Math(m) => &m.style,
        }
    }

    /// Mutable style.
    pub fn style_mut(&mut self) -> &mut Style {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => &mut s.style,
            Self::Arrow(c) | Self::Line(c) => &mut c.style,
            Self::Freedraw(f) => &mut f.style,
            Self::Math(m) => &mut m.style,
        }
    }

    /// Group ids, innermost first.
    pub fn groups(&self) -> &[String] {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => &s.groups,
            Self::Arrow(c) | Self::Line(c) => &c.groups,
            Self::Freedraw(f) => &f.groups,
            Self::Math(m) => &m.groups,
        }
    }

    /// Mutable group ids.
    pub fn groups_mut(&mut self) -> &mut Vec<String> {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => &mut s.groups,
            Self::Arrow(c) | Self::Line(c) => &mut c.groups,
            Self::Freedraw(f) => &mut f.groups,
            Self::Math(m) => &mut m.groups,
        }
    }

    /// Rewrites hex colour literals as lowercase `#rrggbb`.
    pub fn normalise_colours(&mut self) {
        self.style_mut().normalise_colours();
    }

    /// Whether the node is locked.
    pub fn locked(&self) -> bool {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => s.locked,
            Self::Arrow(c) | Self::Line(c) => c.locked,
            Self::Freedraw(f) => f.locked,
            Self::Math(m) => m.locked,
        }
    }

    /// The human-visible text of the node: a label, text content, frame
    /// title or formula.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => s.label.as_deref(),
            Self::Arrow(c) | Self::Line(c) => c.label.as_deref(),
            Self::Freedraw(_) => None,
            Self::Math(m) => Some(&m.source),
        }
    }

    /// Moves the node by `(dx, dy)`.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => {
                s.x += dx;
                s.y += dy;
            }
            Self::Arrow(c) | Self::Line(c) => {
                for p in &mut c.points {
                    p.x += dx;
                    p.y += dy;
                }
            }
            Self::Freedraw(f) => {
                for p in &mut f.points {
                    p.x += dx;
                    p.y += dy;
                }
            }
            Self::Math(m) => {
                m.x += dx;
                m.y += dy;
            }
        }
    }

    /// Every number in the node, for validation.
    fn numbers(&self) -> Vec<f64> {
        let style = self.style();
        let mut out: Vec<f64> = [
            style.stroke_width,
            style.opacity,
            style.font_size,
            style.fill_opacity,
        ]
        .into_iter()
        .flatten()
        .collect();
        match self {
            Self::Rect(s)
            | Self::Diamond(s)
            | Self::Ellipse(s)
            | Self::Text(s)
            | Self::Frame(s)
            | Self::Image(s)
            | Self::Mermaid(s) => out.extend([s.x, s.y, s.width, s.height, s.angle]),
            Self::Arrow(c) | Self::Line(c) => {
                out.extend(c.points.iter().flat_map(|p| [p.x, p.y]));
                for b in [&c.start, &c.end].into_iter().flatten() {
                    if let Some(a) = b.anchor {
                        out.extend([a.x, a.y]);
                    }
                }
            }
            Self::Freedraw(f) => {
                out.push(f.size);
                out.extend(f.points.iter().flat_map(|p| [p.x, p.y, p.pressure]));
            }
            Self::Math(m) => out.extend([m.x, m.y, m.size, m.width, m.height]),
        }
        out
    }

    /// Checks the invariants a node must hold on its own: finite numbers,
    /// a non-empty id, two or more connector points, the fields its kind
    /// needs.
    pub fn validate(&self) -> Result<(), FigureError> {
        let id = self.id();
        let bad = |why: &str| Err(FigureError::InvalidArgument(format!("node `{id}`: {why}")));
        if id.is_empty() {
            return Err(FigureError::InvalidArgument(
                "a node id must not be empty".to_owned(),
            ));
        }
        if self.numbers().iter().any(|n| !n.is_finite()) {
            return bad("every number must be finite");
        }
        if let Some(c) = self
            .style()
            .colours()
            .find(|c| crate::palette::is_malformed_hex(c))
        {
            return bad(&format!("`{c}` is not a valid #rrggbb colour"));
        }
        if let Self::Math(m) = self
            && !(m.size.is_finite() && m.size > 0.0 && m.size <= 1024.0)
        {
            return bad("a formula's size must be between 0 and 1024");
        }
        match self {
            Self::Arrow(c) | Self::Line(c) if c.points.len() < 2 => {
                bad("a connector needs at least two points")
            }
            Self::Image(s) if s.src.as_deref().is_none_or(str::is_empty) => {
                bad("an image needs `src`")
            }
            Self::Mermaid(s) if s.source.is_none() => bad("a Mermaid block needs `source`"),
            _ => Ok(()),
        }
    }
}

/// The complete figure scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigureScene {
    /// Format version (2).
    pub version: u32,
    /// Page width: the export region when there is no frame.
    pub width: f64,
    /// Page height.
    pub height: f64,
    /// Page background: a palette token or `#rrggbb`. Absent: transparent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Nodes in paint order (first is bottom).
    pub nodes: Vec<FigureNode>,
}

impl FigureScene {
    /// An empty scene.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            version: SCENE_VERSION,
            width,
            height,
            background: None,
            nodes: Vec::new(),
        }
    }

    /// Rewrites every hex colour literal as lowercase `#rrggbb`, so a file
    /// written by hand and one written by the picker diff the same way.
    pub fn normalise_colours(&mut self) {
        if let Some(bg) = &mut self.background {
            *bg = crate::palette::normalize_colour(bg);
        }
        for node in &mut self.nodes {
            node.normalise_colours();
        }
    }

    /// The node with `id`.
    pub fn node_by_id(&self, id: &str) -> Option<&FigureNode> {
        self.nodes.iter().find(|n| n.id() == id)
    }

    /// Mutable node with `id`.
    pub fn node_mut(&mut self, id: &str) -> Option<&mut FigureNode> {
        self.nodes.iter_mut().find(|n| n.id() == id)
    }

    /// Index of the node with `id`.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id() == id)
    }

    /// Appends a node.
    pub fn append_node(&mut self, node: FigureNode) {
        self.nodes.push(node);
    }

    /// Replaces the node with the same id; false when there is none.
    pub fn replace_node(&mut self, node: FigureNode) -> bool {
        match self.index_of(node.id()) {
            Some(i) => {
                self.nodes[i] = node;
                true
            }
            None => false,
        }
    }

    /// Removes and returns the node with `id`.
    pub fn remove_node(&mut self, id: &str) -> Option<FigureNode> {
        self.index_of(id).map(|i| self.nodes.remove(i))
    }

    /// The highest counter used by an `n<base36>` id, so new ids never
    /// collide with loaded ones.
    pub fn highest_counter(&self) -> usize {
        self.nodes
            .iter()
            .filter_map(|n| parse_node_id(n.id()))
            .max()
            .unwrap_or(0)
    }

    /// The next free `n<base36>` id.
    pub fn next_node_id(&self) -> String {
        format_node_id(self.highest_counter() + 1)
    }

    /// Checks scene-wide invariants: a sane page, valid nodes, unique ids.
    pub fn validate(&self) -> Result<(), FigureError> {
        if !(self.width.is_finite() && self.height.is_finite())
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err(FigureError::InvalidArgument(
                "the page size must be positive and finite".to_owned(),
            ));
        }
        if let Some(bg) = &self.background
            && !crate::palette::is_colour(bg)
        {
            return Err(FigureError::InvalidArgument(format!(
                "the background `{bg}` is not a palette token or #rrggbb"
            )));
        }
        let mut seen = std::collections::HashSet::new();
        for node in &self.nodes {
            node.validate()?;
            if !seen.insert(node.id()) {
                return Err(FigureError::DuplicateNode(node.id().to_owned()));
            }
        }
        Ok(())
    }

    /// Compact JSON (for IPC).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Reads a scene of any supported version, migrating v1 (ADR 0012).
    pub fn from_json(json: &str) -> Result<Self, FigureError> {
        let value: Value = serde_json::from_str(json)?;
        Self::from_value(value)
    }

    /// [`FigureScene::from_json`] from an already parsed value.
    pub fn from_value(value: Value) -> Result<Self, FigureError> {
        let value = crate::migrate::to_current(value)?;
        let mut scene: FigureScene = serde_json::from_value(value)?;
        scene.normalise_colours();
        scene.validate()?;
        Ok(scene)
    }

    /// The file form: pretty-printed, numbers rounded to two decimals and
    /// whole numbers written without a fraction, final newline.
    pub fn to_file_json(&self) -> Result<String, FigureError> {
        let mut value = serde_json::to_value(self)?;
        tidy_numbers(&mut value);
        let mut text = serde_json::to_string_pretty(&value)?;
        text.push('\n');
        Ok(text)
    }
}

/// Rounds every float in `value` to two decimals; whole numbers become
/// integers so `40.0` is written `40`.
pub fn tidy_numbers(value: &mut Value) {
    match value {
        Value::Number(n) => {
            if let Some(f) = n.as_f64()
                && !n.is_i64()
                && !n.is_u64()
            {
                let r = (f * 100.0).round() / 100.0;
                let r = if r == 0.0 { 0.0 } else { r };
                *value = if r.fract() == 0.0 && r.abs() < 9.0e15 {
                    Value::from(r as i64)
                } else {
                    serde_json::Number::from_f64(r).map_or(Value::Null, Value::Number)
                };
            }
        }
        Value::Array(items) => items.iter_mut().for_each(tidy_numbers),
        Value::Object(map) => map.values_mut().for_each(tidy_numbers),
        _ => {}
    }
}

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// A deterministic base-36 node id, matching TypeScript `formatNodeId`.
pub fn format_node_id(counter: usize) -> String {
    format!("n{}", base36(counter))
}

/// Base-36 digits of `n`.
pub fn base36(mut n: usize) -> String {
    if n == 0 {
        return "0".to_owned();
    }
    let mut buf = Vec::new();
    while n > 0 {
        buf.push(DIGITS[n % 36] as char);
        n /= 36;
    }
    buf.iter().rev().collect()
}

/// The counter of an `n<base36>` id.
pub fn parse_node_id(id: &str) -> Option<usize> {
    let digits = id.strip_prefix('n')?;
    if digits.is_empty() || digits.len() > 10 {
        return None;
    }
    usize::from_str_radix(digits, 36).ok()
}
