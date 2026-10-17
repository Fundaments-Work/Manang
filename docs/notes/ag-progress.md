# Antigravity Engine Progress & Handoff Notes

This log documents progress, commit hashes, public API surfaces, and integration instructions for Claude Code when wiring the engine crates and frontend quality work into the workspace.

---

## Task 1: `crates/manang-buffer`

- **Commit:** `b5dc298`
- **Crate Name:** `manang-buffer`
- **Location:** `crates/manang-buffer`

### Public API Surface
```rust
pub struct DocumentBuffer;

impl DocumentBuffer {
    pub fn new() -> Self;
    pub fn open(initial_text: &str) -> Self;
    pub fn len_chars(&self) -> usize;
    pub fn len_bytes(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn line_count(&self) -> usize;
    pub fn line_len_chars(&self, line: usize) -> Result<usize, BufferError>;

    // Coordinate conversions
    pub fn char_to_line_col(&self, char_idx: usize) -> Result<(usize, usize), BufferError>;
    pub fn line_col_to_char(&self, line: usize, col: usize) -> Result<usize, BufferError>;
    pub fn char_to_position(&self, char_idx: usize) -> Result<Position, BufferError>;
    pub fn position_to_char(&self, pos: Position) -> Result<usize, BufferError>;

    // Byte offsets
    pub fn char_to_byte_idx(&self, char_idx: usize) -> Result<usize, BufferError>;
    pub fn byte_to_char_idx(&self, byte_offset: usize) -> Result<usize, BufferError>;

    // Content operations
    pub fn resolve_range<R: RangeBounds<usize>>(&self, range: R) -> Result<Range<usize>, BufferError>;
    pub fn slice<R: RangeBounds<usize>>(&self, range: R) -> Result<String, BufferError>;
    pub fn insert(&mut self, char_idx: usize, text: &str) -> Result<(), BufferError>;
    pub fn delete<R: RangeBounds<usize>>(&mut self, range: R) -> Result<String, BufferError>;
    pub fn replace<R: RangeBounds<usize>>(&mut self, range: R, text: &str) -> Result<String, BufferError>;

    // History and transactions
    pub fn begin_transaction(&mut self);
    pub fn commit_transaction(&mut self) -> Result<(), BufferError>;
    pub fn rollback_transaction(&mut self) -> Result<(), BufferError>;
    pub fn transaction<T, F>(&mut self, f: F) -> Result<T, BufferError>
    where
        F: FnOnce(&mut DocumentBuffer) -> Result<T, BufferError>;
    pub fn undo(&mut self) -> Result<bool, BufferError>;
    pub fn redo(&mut self) -> Result<bool, BufferError>;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
    pub fn clear_history(&mut self);
}

impl std::fmt::Display for DocumentBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BufferError {
    CharIndexOutOfBounds { char_idx: usize, len_chars: usize },
    ByteOffsetOutOfBounds { byte_offset: usize, len_bytes: usize },
    InvalidUtf8Boundary { byte_offset: usize },
    InvalidRange { start: usize, end: usize },
    LineOutOfBounds { line: usize, line_count: usize },
    ColumnOutOfBounds { line: usize, col: usize, line_len_chars: usize },
    NoActiveTransaction,
    TransactionInProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edit { ... }

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Transaction {
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, Default)]
pub struct History;
```

### Handoff Notes for Claude Code
1. **Workspace Wiring:** To wire `manang-buffer` into the root workspace `Cargo.toml`:
   - Add `"crates/manang-buffer"` to `workspace.members`.
   - Remove the standalone `[workspace]` table from `crates/manang-buffer/Cargo.toml`.
   - Optionally switch `version`, `edition`, `license`, `authors`, `repository`, `rust-version`, `serde`, and `thiserror` to `.workspace = true`.
2. **Safety & Zero-Panic:** All index conversions and slice operations validate against rope boundaries before invoking Ropey methods. Out-of-bounds queries return structured `BufferError` variants without panicking.
3. **Benchmarks:** A Criterion benchmark suite is included at `benches/buffer_throughput.rs`, evaluating insert, delete, and line lookup operations across a 5 MB document.

---

## Task 2: `crates/manang-figure`

- **Commit:** `4926a29`
- **Crate Name:** `manang-figure`
- **Location:** `crates/manang-figure`

### Public API Surface
```rust
pub const SCENE_VERSION: u32 = 1;
pub const SNAP_DISTANCE: f64 = 12.0;

pub struct Vec2 { pub x: f64, pub y: f64 }
pub struct Rect { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }
pub struct StrokeSample { pub x: f64, pub y: f64, pub pressure: f64 }

pub enum AnchorSide { Top, Right, Bottom, Left }
pub struct Anchor { pub x: f64, pub y: f64, pub side: AnchorSide, pub node_id: String }

pub struct StrokeNode { pub id: String, pub points: Vec<StrokeSample>, pub size: f64, pub color: String }
pub struct RectNode { pub id: String, pub x: f64, pub y: f64, pub width: f64, pub height: f64, pub stroke: String, pub fill: Option<String> }
pub struct EllipseNode { pub id: String, pub cx: f64, pub cy: f64, pub rx: f64, pub ry: f64, pub stroke: String, pub fill: Option<String> }
pub struct ArrowNode { pub id: String, pub from: Vec2, pub to: Vec2, pub color: String, pub label: Option<String> }
pub struct MathNode { pub id: String, pub x: f64, pub y: f64, pub tex: String, pub size: f64, pub color: String }

#[serde(tag = "kind")]
pub enum FigureNode {
    Stroke(StrokeNode),
    Rect(RectNode),
    Ellipse(EllipseNode),
    Arrow(ArrowNode),
    Math(MathNode),
}

pub struct FigureScene {
    pub version: u32,
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<FigureNode>,
}

impl FigureScene {
    pub fn new(width: f64, height: f64) -> Self;
    pub fn node_by_id(&self, id: &str) -> Option<&FigureNode>;
    pub fn append_node(&mut self, node: FigureNode);
    pub fn replace_node(&mut self, node: FigureNode) -> bool;
    pub fn remove_node(&mut self, id: &str) -> Option<FigureNode>;
    pub fn to_json(&self) -> Result<String, serde_json::Error>;
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error>;
    pub fn to_svg(&self) -> String;
}

// Geometry & rendering
pub fn rect_center(rect: &Rect) -> Vec2;
pub fn distance(a: Vec2, b: Vec2) -> f64;
pub fn normalize_rect(x: f64, y: f64, width: f64, height: f64) -> Rect;
pub fn union_rect(a: &Rect, b: &Rect) -> Rect;
pub fn node_bounds(node: &FigureNode) -> Rect;
pub fn scene_bounds(scene: &FigureScene) -> Rect;
pub fn hit_test(scene: &FigureScene, point: Vec2) -> Option<&FigureNode>;
pub fn anchors_of(node: &FigureNode) -> Vec<Anchor>;
pub fn all_anchors(scene: &FigureScene, exclude: &[&str]) -> Vec<Anchor>;
pub fn snap_to_anchors(point: Vec2, anchors: &[Anchor], threshold: f64) -> SnapResult;
pub fn arrow_head(from: Vec2, to: Vec2, size: f64) -> [Vec2; 4];
pub fn polygon_to_path(points: &[Vec2]) -> String;
pub fn stroke_to_path(node: &StrokeNode) -> String;
pub fn format_coord(value: f64) -> String;
pub fn format_node_id(counter: usize) -> String;
pub fn render_svg(scene: &FigureScene, options: &SvgExportOptions) -> String;
```

### Handoff Notes for Claude Code
1. **Workspace Wiring:** To wire `manang-figure` into the root workspace `Cargo.toml`:
   - Add `"crates/manang-figure"` to `workspace.members`.
   - Remove the standalone `[workspace]` table from `crates/manang-figure/Cargo.toml`.
   - Dependencies: uses `perfect_freehand = "0.1.1"`, `serde`, `serde_json`, `thiserror`.
2. **Determinism:** `format_coord` uses stable 1-decimal float formatting without floating-point drift, ensuring byte-identical SVG outputs suitable for headless document compilation (e.g. into Typst or Tectonic images).
3. **Snapshot & Testing:** Snapshot tests using `insta` and property tests using `proptest` validate both headless SVG generation and JSON round-tripping.

---

## Task 3: Light-Theme WCAG Contrast Calibration

- **Files Modified/Created:**
  - `src/app.css` (`.theme-light` `--mng-accent`)
  - `docs/adr/0002-light-theme-contrast.md`
  - `src/lib/theme/contrast.test.ts`
- **Audit Findings:**
  - The previous mix `color-mix(in srgb, #c6f135 40%, #1c1c1c)` (`#607126`) reached only 4.36:1 on `--mng-surface-muted` (`#e8e7e2`), failing WCAG AA (4.5:1) for text.
  - Calibrated the ratio to `color-mix(in srgb, #c6f135 35%, #1c1c1c)` (`#586725`), achieving $\ge 5.02:1$ across all light surfaces for text and well exceeding 3.0:1 for UI elements.
  - `--mng-accent-ink` (`#faf9f7`) on `--mng-accent` achieves 5.90:1 contrast.
  - Primary text `--mng-text` achieves $> 13.7:1$ (exceeds WCAG AAA 7:1) across all light surfaces.
- **Automated Verification:** Added `src/lib/theme/contrast.test.ts` to Vitest, which dynamically reads `src/app.css`, extracts the `.theme-light` tokens, and computes exact WCAG 2.1 relative luminance and contrast ratios.

