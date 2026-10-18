use serde::{Deserialize, Serialize};

/// Version tag for the figure scene format.
pub const SCENE_VERSION: u32 = 1;

/// A 2D vector in scene coordinates (1 unit = 1 CSS pixel at 100% zoom).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

/// A rectangle in scene coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// X coordinate of origin.
    pub x: f64,
    /// Y coordinate of origin.
    pub y: f64,
    /// Width of the rectangle.
    pub width: f64,
    /// Height of the rectangle.
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

    /// An empty rectangle with zero dimensions at (0, 0).
    pub const EMPTY: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };
}

/// A sampled point on a freehand path with pen pressure.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokeSample {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Pressure value (0.0 to 1.0).
    pub pressure: f64,
}

impl StrokeSample {
    /// Creates a new `StrokeSample`.
    pub const fn new(x: f64, y: f64, pressure: f64) -> Self {
        Self { x, y, pressure }
    }
}

/// Perimeter attachment side for arrow anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorSide {
    /// Top perimeter edge.
    Top,
    /// Right perimeter edge.
    Right,
    /// Bottom perimeter edge.
    Bottom,
    /// Left perimeter edge.
    Left,
}

/// An attachment point on a node's perimeter where arrows magnetically snap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// X position of the anchor in scene coordinates.
    pub x: f64,
    /// Y position of the anchor in scene coordinates.
    pub y: f64,
    /// Which perimeter side this anchor belongs to.
    pub side: AnchorSide,
    /// ID of the parent node.
    #[serde(rename = "nodeId")]
    pub node_id: String,
}

/// A freehand pressure-tapered vector stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeNode {
    /// Unique node ID.
    pub id: String,
    /// Sampled path points with pressure.
    pub points: Vec<StrokeSample>,
    /// Base stroke diameter in scene units.
    pub size: f64,
    /// Hex stroke color.
    pub color: String,
}

/// A sharp mathematical rectangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RectNode {
    /// Unique node ID.
    pub id: String,
    /// Top-left X.
    pub x: f64,
    /// Top-left Y.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
    /// Hex stroke color.
    pub stroke: String,
    /// Optional hex fill color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
}

/// An ellipse geometry node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EllipseNode {
    /// Unique node ID.
    pub id: String,
    /// Center X.
    pub cx: f64,
    /// Center Y.
    pub cy: f64,
    /// Radius X.
    pub rx: f64,
    /// Radius Y.
    pub ry: f64,
    /// Hex stroke color.
    pub stroke: String,
    /// Optional hex fill color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
}

/// A directional connector arrow with optional label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowNode {
    /// Unique node ID.
    pub id: String,
    /// Starting coordinate.
    pub from: Vec2,
    /// Ending coordinate.
    pub to: Vec2,
    /// Hex color.
    pub color: String,
    /// Optional centered text label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A stamped mathematical formula node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathNode {
    /// Unique node ID.
    pub id: String,
    /// Anchor X coordinate.
    pub x: f64,
    /// Anchor Y coordinate.
    pub y: f64,
    /// Raw TeX / Typst math formula string.
    pub tex: String,
    /// Font size in scene units.
    pub size: f64,
    /// Hex text color.
    pub color: String,
}

/// Polymorphic scene graph node tagged with `"kind"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FigureNode {
    /// Freehand stroke.
    #[serde(rename = "stroke")]
    Stroke(StrokeNode),
    /// Rectangle.
    #[serde(rename = "rect")]
    Rect(RectNode),
    /// Ellipse.
    #[serde(rename = "ellipse")]
    Ellipse(EllipseNode),
    /// Arrow connector.
    #[serde(rename = "arrow")]
    Arrow(ArrowNode),
    /// Stamped math formula.
    #[serde(rename = "math")]
    Math(MathNode),
}

impl FigureNode {
    /// Returns the unique node ID.
    pub fn id(&self) -> &str {
        match self {
            FigureNode::Stroke(s) => &s.id,
            FigureNode::Rect(r) => &r.id,
            FigureNode::Ellipse(e) => &e.id,
            FigureNode::Arrow(a) => &a.id,
            FigureNode::Math(m) => &m.id,
        }
    }

    /// Moves the node by `(dx, dy)` scene units. Every coordinate shifts, so
    /// sizes, radii and stroke pressure are unchanged.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        match self {
            FigureNode::Stroke(s) => {
                for p in &mut s.points {
                    p.x += dx;
                    p.y += dy;
                }
            }
            FigureNode::Rect(r) => {
                r.x += dx;
                r.y += dy;
            }
            FigureNode::Ellipse(e) => {
                e.cx += dx;
                e.cy += dy;
            }
            FigureNode::Arrow(a) => {
                a.from.x += dx;
                a.from.y += dy;
                a.to.x += dx;
                a.to.y += dy;
            }
            FigureNode::Math(m) => {
                m.x += dx;
                m.y += dy;
            }
        }
    }
}

/// The complete Figure scene data model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigureScene {
    /// Schema version (always 1).
    pub version: u32,
    /// Canvas width in scene units.
    pub width: f64,
    /// Canvas height in scene units.
    pub height: f64,
    /// List of ordered nodes (painted from first to last).
    pub nodes: Vec<FigureNode>,
}

impl FigureScene {
    /// Creates a new empty figure scene.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            version: SCENE_VERSION,
            width,
            height,
            nodes: Vec::new(),
        }
    }

    /// Finds a node by ID.
    pub fn node_by_id(&self, id: &str) -> Option<&FigureNode> {
        self.nodes.iter().find(|n| n.id() == id)
    }

    /// Appends a new node to the scene.
    pub fn append_node(&mut self, node: FigureNode) {
        self.nodes.push(node);
    }

    /// Replaces an existing node matching `node.id()`.
    /// Returns true if a node was replaced, false otherwise.
    pub fn replace_node(&mut self, node: FigureNode) -> bool {
        let id = node.id().to_owned();
        if let Some(pos) = self.nodes.iter().position(|n| n.id() == id) {
            self.nodes[pos] = node;
            true
        } else {
            false
        }
    }

    /// Removes a node by ID. Returns the removed node if found.
    pub fn remove_node(&mut self, id: &str) -> Option<FigureNode> {
        if let Some(pos) = self.nodes.iter().position(|n| n.id() == id) {
            Some(self.nodes.remove(pos))
        } else {
            None
        }
    }

    /// Serializes the scene to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserializes a scene from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Generates a deterministic base-36 node ID matching TypeScript `formatNodeId`.
pub fn format_node_id(counter: usize) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if counter == 0 {
        return "n0".to_string();
    }
    let mut num = counter;
    let mut buf = Vec::new();
    while num > 0 {
        buf.push(DIGITS[num % 36]);
        num /= 36;
    }
    buf.reverse();
    format!("n{}", std::str::from_utf8(&buf).unwrap_or("0"))
}
