//! # manang-figure
//!
//! A typed SVG scene graph and deterministic headless renderer for Manang.
//!
//! Features:
//! - Exact mirror of the frontend TypeScript scene graph (`scene.ts`, `geometry.ts`).
//! - Freehand pressure-tapered strokes using `perfect-freehand`.
//! - Sharp mathematical shapes (rectangles, ellipses), snap anchors, and directional arrows.
//! - Stamped math expressions.
//! - Deterministic headless SVG export with stable IDs and float precision.
//! - Lossless JSON round-tripping for git persistence.

#![deny(missing_docs)]

mod error;
mod geometry;
pub mod plugin;
mod scene;
mod store;
mod svg;

pub use error::FigureError;
pub use geometry::{
    SNAP_DISTANCE, SnapResult, all_anchors, anchors_of, arrow_head, distance, format_coord,
    hit_test, node_bounds, normalize_rect, polygon_to_path, rect_center, round_coord, scene_bounds,
    snap_to_anchors, stroke_to_path, union_rect,
};
pub use plugin::{FigurePlugin, manifest, plugin, plugin_with_store};
pub use scene::{
    Anchor, AnchorSide, ArrowNode, EllipseNode, FigureNode, FigureScene, MathNode, Rect, RectNode,
    SCENE_VERSION, StrokeNode, StrokeSample, Vec2, format_node_id,
};
pub use store::{
    ApplyResult, DEFAULT_MAX_HISTORY, DEFAULT_SCENE_HEIGHT, DEFAULT_SCENE_WIDTH, FigureDocument,
    FigureId, FigureInfo, FigureOp, FigureStore, SavedPaths, resolve_figure_paths,
};
pub use svg::{SvgExportOptions, escape_xml, render_svg};
