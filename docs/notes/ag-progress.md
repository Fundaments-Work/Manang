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

---

## Task 4: UI Coverage Expansion Across Seven Screens

- **Files Modified/Created:**
  - `scripts/verify-ui.mjs` (extended with 36 headless CDP checks covering all seven screens)
  - `src/lib/components/screens.test.ts` (10 Vitest architectural and route contracts tests)
  - `e2e/screens.spec.ts` (Playwright end-to-end test suite)
  - `docs/notes/ag-findings.md` (report on unbound method tear-offs in `DeskHeader.svelte`)
- **Coverage Details:**
  1. **Command Palette (`Cmd+K` / `/`):**
     - Slash key `/` from Desk opens the palette dialog.
     - Deep-linked query pre-population (`#palette/cit`).
     - Fuzzy matching and ranking (top match selected).
     - Keyboard navigation with `ArrowDown` / `ArrowUp` selection movement.
     - Section separation: AI Actions before Plugins & Commands.
     - `Escape` dismisses the palette and restores focus.
  2. **Document Switching:**
     - In Library Shelf, clicking a document (e.g. `01_formalism.typ`) activates the document and transfers `aria-current="true"`.
     - Active document border carries the 2px Acid lime boundary (`rgb(198, 241, 53)`).
     - Tree filtering works via `#library-filter`.
  3. **Vector Ink Overlay (`/draw`):**
     - Breadcrumb bar displays exact figure path `figures / vectors / state_lattice.svg`.
     - Pen tray tool switching activates Geometry, Arrow, and Math modes (`aria-pressed="true"`).
     - Drawing with pen commits stroke nodes to the typed SVG scene graph.
     - Done button closes the overlay and restores the Desk.
  4. **Slide Studio & Carousel (`#slides`):**
     - Deep-link selects slide (`#slides/2` -> Slide 3 / 6).
     - 16:9 aspect ratio enforcement on slide frame.
     - Carousel renders all 6 slide preview buttons.
     - Clicking a carousel thumbnail navigates directly to that slide.
     - Bidirectional navigation with `ArrowRight` and `ArrowLeft`.
     - Close button dismisses presentation studio and returns to the Desk.
  5. **Desk, Instrument Shelf, Zen Mode & Overlays:**
     - Margin whisper 4px square gutter placement on broken reference line (`@landauer1961`).
     - Essential instruments cannot be disabled; plugin instruments toggle.
     - Zen mode dissolves header, footer, and nav, presenting a 680px reading column.
     - Overlays preserve background clarity without artificial brightness darkening.


---

## Phase 2 - Task 1: `crates/manang-figure` Native Plugin Module

- **Crate Name:** `manang-figure`
- **Location:** `crates/manang-figure`
- **Manifest:** `manifest.toml` (tier: `native`, name: `figure`, permissions: `filesystem = ["write:project"]`)

### Public API Surface
```rust
pub fn manifest() -> Result<manang_kernel::Manifest, manang_kernel::ManifestError>;
pub fn plugin() -> impl manang_kernel::Plugin;
pub fn plugin_with_store(store: Arc<FigureStore>) -> impl manang_kernel::Plugin;

pub struct FigurePlugin { ... }
pub struct FigureStore { ... }
pub struct FigureDocument { ... }

pub type FigureId = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigureInfo {
    pub id: FigureId,
    pub path: Option<String>,
    pub nodes: usize,
    pub revision: u64,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyResult {
    pub revision: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FigureOp {
    AddNode { node: FigureNode },
    UpdateNode { node: FigureNode },
    RemoveNode { id: String },
    Clear,
    SetStyle {
        id: String,
        stroke: Option<String>,
        fill: Option<String>,
        size: Option<f64>,
        color: Option<String>,
    },
    Undo,
    Redo,
}

pub fn resolve_figure_paths(path: &Path) -> (PathBuf, PathBuf);
```

### Contributed Commands & Argument/Return Shapes
1. `figure.create`:
   - Args: `{ width?: f64, height?: f64 }` (defaults: `800.0`, `600.0`)
   - Return: `Payload::Json({ "id": u64 })`
2. `figure.open`:
   - Args: `{ path: string }`
   - Return: `Payload::Json({ "id": u64 })`
3. `figure.save`:
   - Args: `{ id?: u64, path?: string }`
   - Output: Saves `<stem>.figure.json` scene and companion deterministic headless `<stem>.svg` beside it.
   - Return: `Payload::Empty`
4. `figure.list`:
   - Args: `Payload::Empty` or `{}`
   - Return: `Payload::Json(Vec<FigureInfo>)`
5. `figure.apply`:
   - Args: `{ id?: u64, op: FigureOp }` (supports tagged enum with `"op"`, `"type"`, or `"kind"`, plus string `"undo"` / `"redo"` / `"clear"`)
   - Return: `Payload::Json({ "revision": u64, "applied": bool })`
6. `figure.get`:
   - Args: `{ id?: u64 }` (defaults to active figure)
   - Return: `Payload::Json(FigureScene)`
7. `figure.export-svg`:
   - Args: `{ id?: u64 }`
   - Return: `Payload::Binary(Arc<[u8]>)` (raw UTF-8 SVG buffer, zero-copy across IPC)
8. `figure.hit-test`:
   - Args: `{ id?: u64, x: f64, y: f64 }`
   - Return: `Payload::Json({ "hit": bool, "node_id": Option<String>, "node": Option<FigureNode> })`

### Handoff Notes for Claude Code
1. **Registration in `src-tauri`:**
   ```rust
   kernel.register(
       manang_figure::manifest().expect("figure manifest"),
       Arc::new(manang_figure::plugin()),
   ).expect("register figure plugin");
   ```
2. **Companion SVG Export:** `figure.save` automatically writes both `<name>.figure.json` and deterministic headless `<name>.svg` directly beside it for immediate inclusion in Typst documents (`#image("circuit.svg")`).
3. **Bounded Undo/Redo & Revision:** Each figure document maintains in-memory history snapshots (default 100 steps) and a monotonically increasing revision counter.
4. **Verification & Testing:**
   - 9 integration tests in `crates/manang-figure/tests/kernel.rs` testing all commands against a live `manang_kernel::Kernel`.
   - 2 property tests in `crates/manang-figure/tests/plugin_proptest.rs` validating that arbitrary operation sequences never panic and that undo/redo round-trips state losslessly.
   - Snapshot test validating deterministic SVG output exported through the plugin.

---

## Phase 2 - Task 2: `crates/manang-editor` DocumentBuffer & Undo/Redo/Transactions

- **Crate Name:** `manang-editor`
- **Location:** `crates/manang-editor`
- **Underlying Buffer:** `manang_buffer::DocumentBuffer` (Rope-backed with bounded transaction history and coordinate mapping)

### Public API Surface
```rust
impl Editor {
    // Existing methods preserved:
    pub fn new() -> Self;
    pub fn create(&self) -> DocId;
    pub fn open(&self, path: &Path) -> Result<DocId, EditorError>;
    pub fn save(&self, id: Option<DocId>, path: Option<&Path>) -> Result<(), EditorError>;
    pub fn activate(&self, id: DocId) -> Result<(), EditorError>;
    pub fn insert(&self, id: Option<DocId>, at: usize, text: &str) -> Result<u64, EditorError>;
    pub fn delete(&self, id: Option<DocId>, from: usize, to: usize) -> Result<u64, EditorError>;
    pub fn text(&self, id: Option<DocId>) -> Result<String, EditorError>;
    pub fn info(&self, id: Option<DocId>) -> Result<DocInfo, EditorError>;
    pub fn list(&self) -> Vec<DocInfo>;

    // New methods added:
    pub fn replace(&self, id: Option<DocId>, from: usize, to: usize, text: &str) -> Result<u64, EditorError>;
    pub fn undo(&self, id: Option<DocId>) -> Result<u64, EditorError>;
    pub fn redo(&self, id: Option<DocId>) -> Result<u64, EditorError>;
    pub fn slice(&self, id: Option<DocId>, from: usize, to: usize) -> Result<String, EditorError>;
    pub fn line_col(&self, id: Option<DocId>, char_idx: usize) -> Result<(usize, usize), EditorError>;
}

impl BufferService for Editor {
    fn read_active(&self) -> Result<String, String>;
    fn write_active(&self, text: &str) -> Result<(), String>;
}
```

### Contributed Commands & Argument/Return Shapes
1. `editor.undo`:
   - Args: `{ id?: u64 }` (defaults to active document)
   - Return: `Payload::Json({ "revision": u64 })`
2. `editor.redo`:
   - Args: `{ id?: u64 }`
   - Return: `Payload::Json({ "revision": u64 })`
3. `editor.replace`:
   - Args: `{ id?: u64, from: usize, to: usize, text: string }`
   - Behavior: Atomic transaction recording single undo/redo step.
   - Return: `Payload::Json({ "revision": u64 })`
4. `editor.slice`:
   - Args: `{ id?: u64, from: usize, to: usize }`
   - Return: `Payload::Json(Value::String)`
5. `editor.line-col`:
   - Args: `{ id?: u64, char: usize }`
   - Return: `Payload::Json({ "line": usize, "column": usize })` (0-based)

### Criterion Benchmark Results (5 MB Document)
Evaluated across a live 5 MB UTF-8 document in `benches/editor_throughput.rs`:
- **`insert_middle_5mb`:** $476.00\text{ ns}$ ($< 0.5\text{ \textmu s}$)
- **`delete_middle_5mb`:** $1.13\text{ \textmu s}$
- **`undo_middle_5mb`:** $437.16\text{ ns}$ ($< 0.5\text{ \textmu s}$)
All operations stay well below the 8 ms keystroke latency SLA.

### Handoff Notes for Claude Code
1. `DocumentBuffer::write_to` streams chunked rope slices directly to disk, avoiding intermediate allocations.
2. The `BufferService` implementation remains 100% compatible for other plugins (such as Typst and WASM plugins).
3. The oracle proptest against `Vec<char>` passes with zero regressions.
---

## Phase 2 - Task 3: `crates/manang-markdown` Native Plugin

- **Crate Name:** `manang-markdown`
- **Location:** `crates/manang-markdown`
- **Manifest:** `manifest.toml` (tier: `native`, name: `markdown`)

### Public API Surface
```rust
pub fn manifest() -> Result<manang_kernel::Manifest, manang_kernel::ManifestError>;
pub fn plugin() -> impl manang_kernel::Plugin;

pub struct MarkdownPlugin { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOptions {
    pub allow_raw_html: bool,
    pub allowed_html_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    pub title: String,
    pub level: u32,
    pub line: usize,
    pub char_offset: usize,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub line: Option<usize>,
}

pub fn render_to_html(source: &str, options: &RenderOptions) -> (String, Vec<Diagnostic>);
pub fn extract_frontmatter(source: &str) -> (Option<serde_json::Value>, &str);
pub fn extract_outline(source: &str) -> Vec<Heading>;
pub fn markdown_to_typst(source: &str) -> (String, Vec<Diagnostic>);
```

### Contributed Commands & Shapes
1. `markdown.render-html`:
   - Args: `{ source: string, options?: { allow_raw_html?: bool, allowed_html_tags?: string[] } }`
   - Return: `Payload::Json({ html: string, frontmatter: Option<Value>, headings: Heading[], diagnostics: Diagnostic[] })`
   - Features: Parses YAML (`---`) and TOML (`+++`) metadata blocks, tables, footnotes, task lists, strikethrough, preserves inline math (`<span class="math math-inline">$...$</span>`) and display math (`<div class="math math-display">$$...$$</div>`).
   - XSS Hardening: Strict sanitization against XSS vectors. Raw HTML tags are escaped by default (`&lt;...&gt;`). Unsafe URL schemes (`javascript:`, `vbscript:`, `data:text/html`) in links and images are replaced with `about:blank`. When `allow_raw_html` is explicitly enabled, only tags matching `allowed_html_tags` are permitted, and all event handlers (`onerror`, `onload`, `onclick`, `onmouseover`, `onfocus`, `onblur`) are detected and escaped with warnings in diagnostics.
2. `markdown.outline`:
   - Args: `{ source: string }`
   - Return: `Payload::Json(Heading[])`
   - Features: Fast AST pass returning heading title, level (1..6), 1-based line number, character offset, and custom heading attributes/id if present.
3. `markdown.to-typst`:
   - Args: `{ source: string }`
   - Return: `Payload::Json({ typst: string, diagnostics: Diagnostic[] })`
   - Supported Typst subset:
     - Headings (`= ...` to `====== ...`)
     - Paragraphs, bold (`*...*`), italics (`_..._`), strikethrough (`#strike[...]`)
     - Blockquotes (`#quote(block: true)[...]`)
     - Ordered (`+ `) and unordered (`- `) lists (with nested indent) and task lists (`[x] ` / `[ ] `)
     - Fenced code blocks (` ```lang ... ``` `) and inline code (`` `...` ``)
     - Inline and display math (`$...$`, `$ ... $`)
     - Hyperlinks (`#link("url")[text]`) and images (`#image("url")`)
     - Tables (`#table(columns: n, [...])`)
     - Footnotes (`#footnote[...]`) and horizontal rules (`#line(length: 100%)`)
   - Non-silent diagnostics: Unsupported constructs (e.g. raw HTML tags) emit warnings with exact line numbers in `diagnostics` instead of silently dropping content.

### Handoff Notes for Claude Code
1. **Registration in `src-tauri`:**
   ```rust
   kernel.register(
       manang_markdown::manifest().expect("markdown manifest"),
       Arc::new(manang_markdown::plugin()),
   ).expect("register markdown plugin");
   ```
2. **Deterministic Sanitization:** No third-party heavy HTML sanitizers needed; pulldown-cmark AST event interception ensures zero-allocation scanning and secure XSS mitigation.
3. **Tests:**
   - 7 kernel integration tests in `crates/manang-markdown/tests/kernel.rs` covering all commands, frontmatter parsing, XSS vectors, and Typst generation.
   - Property tests with `proptest` in `crates/manang-markdown/tests/fuzz_proptest.rs` verifying arbitrary byte/string inputs never panic.
   - `insta` snapshot tests capturing stable HTML and Typst generation.
---

## Phase 2 - Task 4: `crates/manang-citations` Native Plugin

- **Crate Name:** `manang-citations`
- **Location:** `crates/manang-citations`
- **Manifest:** `manifest.toml` (tier: `native`, name: `citations`, permissions: `network = ["api.crossref.org", "api.openalex.org", "export.arxiv.org"]`, `filesystem = ["read:project"]`)

### Public API Surface
```rust
pub fn manifest() -> Result<manang_kernel::Manifest, manang_kernel::ManifestError>;
pub fn plugin() -> impl manang_kernel::Plugin;
pub fn plugin_with_client(http_client: Arc<dyn HttpClient>) -> impl manang_kernel::Plugin;
pub fn plugin_with_store(index: Arc<CitationIndex>, http_client: Arc<dyn HttpClient>) -> impl manang_kernel::Plugin;

pub struct CitationsPlugin { ... }
pub struct CitationIndex { ... }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub entry_type: String,
    pub title: Option<String>,
    pub authors: Vec<Author>,
    pub year: Option<u32>,
    pub month: Option<u32>,
    pub journal_or_publisher: Option<String>,
    pub volume: Option<String>,
    pub number: Option<String>,
    pub pages: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub arxiv_id: Option<String>,
    pub abstract_text: Option<String>,
    pub raw_fields: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub family: Option<String>,
    pub given: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Difference {
    pub field: String,
    pub local: String,
    pub remote: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    pub matched: Option<Entry>,
    pub differences: Vec<Difference>,
    pub message: Option<String>,
}

pub trait HttpClient: Send + Sync {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, CitationsError>;
}
pub struct BlockingHttpClient { ... }
pub struct MockHttpClient { ... }

pub fn parse_bibtex(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic>;
pub fn parse_csl_json(source: &str, file: Option<&str>) -> Result<Vec<Entry>, ParseDiagnostic>;
pub fn parse_file(path: &Path) -> Result<Vec<Entry>, ParseDiagnostic>;
pub fn format_entry(entry: &Entry, style: &str) -> Result<String, CitationsError>;
pub fn format_apa(entry: &Entry) -> String;
pub fn format_numeric(entry: &Entry) -> String;
pub fn extract_keys(source: &str, syntax: &str) -> Result<Vec<String>, CitationsError>;
pub fn verify_entry(local: &Entry, client: &dyn HttpClient) -> VerificationResult;
```

### Contributed Commands & Shapes
1. `citations.load`:
   - Args: `{ paths: string[] }` or `{ path: string }`
   - Return: `Payload::Json({ loaded: usize, total: usize, errors: ParseDiagnostic[] })`
   - Features: Auto-detects BibTeX/BibLaTeX (`.bib`, `.biblatex`) and CSL-JSON (`.json`, `.csl`), parses into normalized `Entry` models, reports syntax/parsing diagnostics with exact 1-based file, line, and column coordinates.
2. `citations.search`:
   - Args: `{ query: string, limit?: usize }`
   - Return: `Payload::Json({ results: [ { key: string, score: f64, entry: Entry } ] })`
   - Features: Ranked in-memory search scoring across key exact/containment, title words, author family and given names, publication year, and character subsequence fuzzy matching, with deterministic tie-breaking.
3. `citations.get`:
   - Args: `{ key: string }`
   - Return: `Payload::Json(Entry)`
4. `citations.format`:
   - Args: `{ key: string, style?: "apa" | "numeric" }` (default: `"apa"`)
   - Return: `Payload::Json({ key: string, style: string, formatted: string })`
   - Supported styles:
     - `"apa"`: Full APA-compliant author list (initials, ampersand, ellipsis for >20), publication year, italicized/punctuated titles, container/issue/pages, and DOI link.
     - `"numeric"`: IEEE-like numbered citation with quoted titles and abbreviated pagination labels (`[1] A. M. Turing, ...`).
5. `citations.extract-keys`:
   - Args: `{ source: string, syntax: "typst" | "latex" | "markdown" }`
   - Return: `Payload::Json({ keys: string[] })`
   - Features:
     - `typst`: Extracts `@key` identifiers while safely rejecting email addresses (`user@domain.com`).
     - `latex`: Parses `\cite{...}`, `\citep{...}`, `\citet{...}`, etc., handling optional square bracket arguments and comma-delimited key lists.
     - `markdown`: Extracts `[@key1; @key2]`, `[-@key]`, and `@key[p. 10]`.
6. `citations.verify`:
   - Args: `{ key: string }`
   - Return: `Payload::Json({ status: "verified" | "mismatch" | "not_found" | "error", matched: Option<Entry>, differences: Difference[] })`
   - Verification Engine:
     - Crossref resolution by DOI (`https://api.crossref.org/works/{doi}`) with OpenAlex fallback.
     - arXiv resolution by arXiv ID (`https://export.arxiv.org/api/query?id_list={id}`) with OpenAlex fallback.
     - Bibliographic title search query when identifiers are absent.
     - Sandboxed HTTP: Exact-host allowlist (`api.crossref.org`, `api.openalex.org`, `export.arxiv.org`), size limit (2 MB), timeout (10s), strict redirect validation (each hop checked against allowlist), polite User-Agent, and on-disk SHA hash response caching.
     - Offline Testing: `MockHttpClient` for 100% deterministic, zero-network tests.

### Handoff Notes for Claude Code
1. **Registration in `src-tauri`:**
   ```rust
   kernel.register(
       manang_citations::manifest().expect("citations manifest"),
       Arc::new(manang_citations::plugin()),
   ).expect("register citations plugin");
   ```
2. **Deterministic Offline Tests:** All test suites use `MockHttpClient` so `cargo test` never touches the live network.
3. **Tests:**
   - 3 proptests in `tests/fuzz_proptest.rs` verifying arbitrary string inputs to BibTeX, CSL-JSON, and key extraction never panic.
   - 7 kernel integration tests in `tests/kernel.rs` covering commands, search ranking, citation formatting, mock network verification, and 4 `insta` snapshots for formatted output and parse errors with line/column coordinates.

## UI Wiring - Task A: Search View -> `search.*`

### Summary of Changes
1. **Typed Kernel Wrappers (`src/lib/kernel/api.ts`):**
   - Added typed interfaces: `SearchFilesRequest`, `SearchFilesResponse`, `SearchTextRequest`, `SearchTextResponse`, `SearchLineMatch`, `SearchRange`, `SearchContextLine`, `SearchFileMatches`, `SearchStats`, `SearchPreviewRequest`, `SearchPreviewResponse`, `SearchPreviewFile`, `SearchPreviewEdit`, `SearchApplyRequest`, `SearchApplyResponse`, `SearchApplyFileResult`, `SearchInTextRequest`.
   - Added `searchApi(kernel)` wrapper with methods for `files`, `text`, `cancel`, `replacePreview`, `replaceApply`, `inText`.
2. **Browser Mock Plugin (`src/lib/kernel/mockSearch.ts`):**
   - Replicated exact command shapes and signatures from `crates/manang-search`:
     - `search.files`: fuzzy rank across current project's files using `fuzzyRank`.
     - `search.text`: regex / literal search line-by-line with case sensitivity, whole word, regex, include/exclude glob patterns, scalar ranges and 1-based columns, context lines before/after.
     - `search.cancel`: sets cancelled flag and returns `{ cancelled: 1 }`.
     - `search.replace-preview`: plans replacements across matching files with content hash checking and `plan_id`.
     - `search.replace-apply`: verifies content hashes, performs atomic updates in `MockProjectHost`, emits `project.changed`.
     - `search.in-text`: in-document text search.
   - Registered `searchPlugin(host)` in `src/lib/kernel/dev.ts`.
3. **Pure Workbench Utilities (`src/lib/workbench/search.ts`):**
   - Implemented `splitLineByRanges` to cleanly divide matched line text into matching/non-matching segments for exact highlight rendering.
   - Implemented `parsePatternList` for glob filtering.
   - Implemented `formatMatchCount`.
4. **Search Panel UI (`src/lib/components/panels/SearchPanel.svelte`):**
   - Connected directly to `searchApi(kernel)` for both file-name (`search.files`) and content search (`search.text`).
   - Integrated match case (`Aa`), whole word (`\b`), regex (`.*`), and collapsible include/exclude glob filter controls.
   - 200 ms debounce with automatic cancellation of previous in-flight search (`api.cancel()`).
   - Integrated replace controls: replace input field, "Replace all" button via `search.replace-preview` and `search.replace-apply` with per-file status reports and error handling.
   - Collapsible per-file match groups with line numbers and match highlighting via `<mark class="bg-math/25 text-text">`.
   - Result truncation notification when limits are reached.
   - Match clicks navigate to file, line, and column via `goToMatch`.
5. **Testing & Verification:**
   - Unit tests in `src/lib/kernel/mockSearch.test.ts` (4 tests) and `src/lib/workbench/search.test.ts` (6 tests).
   - Updated plugin count and groups in `src/lib/workbench/plugins.test.ts`.
   - Passed `pnpm run typecheck && pnpm run lint && pnpm vitest run` (22 test suites, 209 tests all green).
   - Passed end-to-end headless verification harness (`verify-ui.mjs`, 122/122 checks passing on port 1431).

