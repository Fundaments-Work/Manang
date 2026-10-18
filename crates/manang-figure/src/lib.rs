//! # manang-figure
//!
//! Manang's figure studio core (ADR 0012): a typed, versioned scene graph,
//! ID-addressed invertible operations with delta-based undo, arrow binding
//! and routing, deterministic SVG export in clean and hand-drawn styles,
//! layered auto-layout, Mermaid conversion and the agent tools other
//! plugins and AI agents drive figures with.
//!
//! Pure Rust with no I/O outside the store's file reads and writes, so the
//! scene logic compiles to `wasm32-unknown-unknown`.

#![deny(missing_docs)]

mod error;
mod geometry;
mod layout;
pub mod math;
mod mermaid;
mod migrate;
mod ops;
pub mod palette;
pub mod plugin;
mod route;
mod scene;
pub mod sketch;
mod store;
mod svg;
pub mod text;
pub mod tools;

pub use error::FigureError;
pub use geometry::{
    HIT_TOLERANCE, anchor_of, anchor_point, distance, format_coord, frame_children, freedraw_path,
    hit_test, hits, node_bounds, node_rect, nodes_in_rect, normalize_rect, outline_point,
    points_bounds, polygon_to_path, rotate_point, round_coord, scene_bounds, union_rect,
};
pub use layout::{Direction, layout_moves};
pub use math::{MathError, MathSvg, render_math};
pub use mermaid::{nodes_from_layout, to_mermaid};
pub use ops::{AlignEdge, Axis, Delta, End, FigureOp, ZOrder, apply as apply_op, next_group_id};
pub use palette::Theme;
pub use plugin::{FigurePlugin, manifest, plugin, plugin_with_events, plugin_with_store};
pub use route::{BIND_GAP, elbow, route};
pub use scene::{
    Binding, ConnectorNode, FigureNode, FigureScene, FontFamily, FreedrawNode, Head, Heads,
    MathLanguage, MathNode, Rect, Routing, SCENE_VERSION, ShapeNode, StrokeSample, StrokeStyle,
    Style, TextAlign, Vec2, format_node_id, parse_node_id,
};
pub use store::{
    ApplyResult, DEFAULT_MAX_HISTORY, DEFAULT_SCENE_HEIGHT, DEFAULT_SCENE_WIDTH, FigureDocument,
    FigureId, FigureInfo, FigureStore, SavedPaths, resolve_figure_paths,
};
pub use svg::{SvgExportOptions, escape_xml, export_region, render_svg, render_svg_with};
