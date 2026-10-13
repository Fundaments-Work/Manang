# DESIGN.md — Technical Architecture & System Design Document

**Project:** Manang  
**Document Status:** Approved & Canonical  
**Version:** 1.0.0  
**Target Architecture:** Tauri v2 + Rust Core + React 19 / CodeMirror 6 + Excalidraw (MIT)  

---

## 1. High-Level Architecture & Domain Model

Manang is partitioned into two primary boundaries:
1. **Presentation Boundary (Tauri v2 Webview):** Handles user interactions, editor viewports, visual canvases, and DOM rendering.
2. **Native Core Boundary (Rust Daemon):** Manages file system persistence, buffer trees, incremental compilation, WASM plugin execution, and agentic workflows.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        PRESENTATION BOUNDARY                           │
│                                                                        │
│  ┌───────────────────┐  ┌───────────────────┐  ┌────────────────────┐  │
│  │   File Explorer   │  │   CodeMirror 6    │  │   PDF / SVG Viewer │  │
│  │   & Symbol Tree   │  │  (Rope Viewport)  │  │  (PDF.js & WebGL)  │  │
│  └───────────────────┘  └─────────┬─────────┘  └──────────▲─────────┘  │
│                                   │                       │            │
│  ┌───────────────────┐            │                       │            │
│  │  Excalidraw (MIT) │            │                       │            │
│  │   Vector Canvas   │────────────┼───────────────────────┤            │
│  └───────────────────┘            │                       │            │
│                                   │                       │            │
│  ┌───────────────────┐            │                       │            │
│  │ CeTZ/TikZ Sandbox │────────────┘                       │            │
│  │ Visual Primitives │                                    │            │
│  └───────────────────┘                                    │            │
└───────────────────────────────────┬───────────────────────┼────────────┘
                                    │ Low-latency IPC       │ Binary Stream
┌───────────────────────────────────▼───────────────────────┴────────────┐
│                         NATIVE CORE BOUNDARY                           │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ Workspace Manager (File Watcher + Git `gix` + Config Resolver)   │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │                                  │
│  ┌──────────────────────────────────▼───────────────────────────────┐  │
│  │ Document Buffer Engine (`ropey::Rope` + Tree-sitter Parser)      │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │                                  │
│         ┌───────────────────────────┼───────────────────────────┐      │
│         ▼                           ▼                           ▼      │
│  [Typst Engine]              [Tectonic Engine]          [WASM Plugins] │
│  • Memory AST Cache          • CTAN Resolver            • Extism Host  │
│  • Sub-50ms Incremental      • XeTeX Direct Stream      • Sandboxed ABI│
│         │                           │                           │      │
│         └───────────────────────────┼───────────────────────────┘      │
│                                     ▼                                  │
│                         [Agentic Orchestrator]                         │
│                         • LLM Multi-Provider Gateway                   │
│                         • Crossref/arXiv/OpenAlex Client               │
│                         • Autonomous Compiler Auto-Fixer               │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Core Domain Entities

```rust
// Core Domain Entities (Rust Core)

pub struct Workspace {
    pub root_path: PathBuf,
    pub config: ProjectConfig,
    pub active_document: Option<DocumentId>,
    pub documents: HashMap<DocumentId, Document>,
    pub canvases: HashMap<CanvasId, CanvasScene>,
    pub crdt_session: Arc<CrdtSession>,
}

pub struct Document {
    pub id: DocumentId,
    pub path: PathBuf,
    pub format: DocumentFormat, // Typst | LaTeX | Markdown
    pub buffer: ropey::Rope,
    pub syntax_tree: tree_sitter::Tree,
    pub dirty: bool,
    pub diagnostics: Vec<Diagnostic>,
}

pub enum DocumentFormat {
    Typst,
    Latex,
    Markdown,
}

pub struct CanvasScene {
    pub id: CanvasId,
    pub path: PathBuf,
    pub kind: CanvasKind, // Excalidraw | CetzSandbox | TikzSandbox
    pub raw_json: Vec<u8>,
    pub exported_svg_path: PathBuf,
    pub exported_pdf_path: PathBuf,
}

pub struct Diagnostic {
    pub range: SourceRange,
    pub severity: DiagnosticSeverity, // Error | Warning | Info
    pub message: String,
    pub suggestion: Option<DiffSuggestion>,
}
```

---

## 3. UI/UX Design System & Layout Geometry

Manang is strictly governed by the **Fundaments.work Design System** (`/run/media/sapiens/Development/Fundaments/design-system/tokens.md` and `guidelines.md`): *"Square, quiet, and set in two weights."*

### 3.1. Absolute Visual Invariants
* **Zero Corner Radius (`--radius: 0`):** Every element terminates in sharp 90-degree right angles without exception. No rounded rectangles, no pill buttons, no circular avatars or indicators.
* **Hairline Dividers (`--hairline: 1px`):** Depth is communicated strictly via 1px hairline boundaries (`#3d3d42` dark, `#d4d3ce` light) and tonal surface steps, never via drop shadows, blurs, or ambient glows.
* **Two Typefaces, Two Weights:**
  - **DM Sans (300 Light & 400 Regular):** Interface chrome, section headers, dialogs, and narrative body copy.
    - Display: Sans 300, tracking `-3%` (`-0.03em`)
    - Headline: Sans 300, tracking `-2%` (`-0.02em`)
    - Body: Sans 400, tracking `-1%` (`-0.01em`)
  - **DM Mono (400 Regular):** Anything that is data—code editors, line numbers, status readouts, timestamps, file sizes, compiler metrics, and citekeys.
* **Credit Line:** Every product carries `Made by Fundaments.work` set in DM Mono 400 at 0.35× the wordmark size, in the muted text token.

### 3.2. Color Tokens (Base Layer & State Accent)
* **Dark Base (`:root`, Default):**
  - Background: `#141416`
  - Surface: `#1c1c20`
  - Surface Raised (`--surface-hi`): `#28282c`
  - Line / Hairline: `#3d3d42`
  - Primary Text: `#e8e6e1`
  - Secondary Text (`--text-60`): `#9a9a9a`
  - Muted Text: `#7a7a7a`
  - Hover: `#454549`
* **Light Theme Override (`.theme-light`):**
  - Page Background (`--bg`): `#faf9f7`
  - Surface: `#f4f3f0`
  - Line / Hairline: `#d4d3ce`
  - Primary Text (`--text`): `#1c1c1c`
  - Secondary Text (`--text-60`): `#6b6b6b`
  - Muted (disabled only): `#9c9c9c`
* **Manang Product State Accent:**
  - **Cerulean (`#38bdf8`):** Assigned strictly to compilation status, live cursor synchronization, and active document state. Carries a DM Mono text label alongside it (never color alone).

### 3.3. Layout Geometry: Ergonomic 3-Column Split
* **Left Column (Collapsible, 240px default, `#141416`):**
  - Project file tree with clean icons.
  - Document outline / symbol hierarchy (sections, subsections, figures, tables).
  - Bibliography references panel with citekey counters.
* **Center Column (Flexible, ~50% viewport, `#141416`):**
  - **Code Editor (CodeMirror 6):** Line numbers in DM Mono muted, syntax highlighting (Cerulean `#38bdf8`, Lime `#c6f135`, warm amber), inline hairline diagnostic error badges.
  - **Breadcrumbs:** `workspace / papers / 2026-q1 / document.typ`.
* **Right Column (Collapsible, ~50% viewport, `#1c1c20`):**
  - **Multi-Mode Studio:** Tabbed switcher between **Rendered PDF (Academic 2-Column)**, **CeTZ Visual Sandbox**, **Excalidraw Vector Canvas**, and **Touying Slides**.
  - Synchronized scrolling and **SyncTeX click-to-jump** navigation.
  - Sidekick Agent drawer (Prompt-to-Doc co-author, citation checker).

### 3.4. Command Palette & Navigation
* Global Command Palette accessible via `Cmd+K` / `Ctrl+K`.
* First-class keyboard modes: Standard GUI, **Vim Mode** (via `@replit/codemirror-vim`), and Emacs keymaps.

---

## 4. IPC Interface & Binary Protocol Specification

To prevent serialization lag, all IPC calls adhere to strict schemas:

### 4.1. Tauri Commands (Frontend to Rust)

```typescript
// TypeScript IPC Definitions
interface CompileRequest {
  documentId: string;
  sourceText?: string; // Optional if syncing via Rope buffer
  targetFormat: "pdf" | "svg";
  syncTexTarget?: { line: number; column: number };
}

interface CompileResponse {
  success: boolean;
  diagnostics: Diagnostic[];
  pageCount: number;
  compilationDurationMs: number;
}

// PDF bytes are transferred via raw streaming ArrayBuffers:
// invokeBinary("get_rendered_pdf", { documentId }) -> Promise<ArrayBuffer>
```

### 4.2. Tauri Events (Rust to Frontend)

* `manang://buffer-updated`: Emitted when an external file watcher or agent patch updates the buffer.
* `manang://compile-progress`: Streams compilation status (`idle` -> `evaluating` -> `rendering` -> `ready`).
* `manang://diagnostic-emitted`: Pushes compiler errors and linter alerts with exact line/column spans.
* `manang://agent-token-stream`: Real-time streaming chunks for AI generation and diff suggestions.

---

## 5. Visual Studio & Dual Illustration Engine

```
┌────────────────────────────────────────────────────────┐
│                 VISUAL ASSET PIPELINE                  │
├────────────────────────────────────────────────────────┤
│ 1. Freeform Sketching (Excalidraw MIT)                 │
│    • User draws on canvas -> scene saved to .excalidraw│
│    • Headless worker parses scene -> generates:        │
│      ├── figures/my_sketch.svg                         │
│      └── figures/my_sketch.pdf                         │
│    • Auto-inserts snippet into document                │
├────────────────────────────────────────────────────────┤
│ 2. Mathematical Vector Studio (CeTZ & TikZ)            │
│    • Live isolated sandbox compiles snippet in <20ms   │
│    • Interactive coordinate & geometry sliders         │
│    • Library of scientific templates:                  │
│      - State machines, Petri nets, Markov chains       │
│      - Deep learning architectures (CNN, Transformer)  │
│      - Commutative algebraic diagrams                  │
└────────────────────────────────────────────────────────┘
```

---

## 6. Scientific Generative Agent Architecture

The built-in agent runs as an autonomous multi-stage pipeline:

```
[ Natural Language Request ]
            │
            ▼
┌───────────────────────┐
│     Planner Agent     │ -> Selects template (IEEE / Nature / Touying Slides)
└───────────┬───────────┘
            │
            ▼
┌───────────────────────┐
│    Authoring Agent    │ -> Generates Typst / LaTeX / CeTZ content
└───────────┬───────────┘
            │
            ▼
┌───────────────────────┐
│ Compilation Validator │ -> Compiles code with Typst / Tectonic crate
└───────────┬───────────┘
            │
      ┌─────┴─────────────────────────┐
      │ Clean                         │ Compiler Error
      ▼                               ▼
[ Stream Result ]           ┌───────────────────┐
                            │ Self-Healing Loop │ -> Analyzes diagnostic
                            └─────────┬─────────┘    generates 1-click diff patch
                                      │
                                      └──► (Re-evaluates)
```

### 6.1. Scholarly Citation Verification Engine
* Extracts citation identifiers (`\cite{Smith2024}`, `@smith_quantum_2024`).
* Connects asynchronously to **OpenAlex**, **Crossref**, and **arXiv** via Tokio.
* Verifies:
  1. Does the DOI / paper exist?
  2. Does the title, author, and year match?
  3. Does the text claim align with the paper's indexed abstract? Flags potential hallucinations.

---

## 7. WASM Plugin Architecture (Extism Host)

Plugins are isolated WebAssembly modules compiled to `.wasm`:

```rust
// Plugin Manifest Definition (manifest.toml)
[plugin]
name = "vale-linter"
version = "0.1.0"
entrypoint = "plugin.wasm"

[capabilities]
read_buffer = true
write_diagnostics = true
network_whitelist = []
filesystem_whitelist = []
```

### 7.1. Plugin Host Hooks
* `init(config: &str) -> Result<()>`
* `on_lint_document(document_text: &str, format: &str) -> Vec<Diagnostic>`
* `on_transform_ast(ast_json: &str) -> String`
* `register_tools() -> Vec<McpToolDefinition>`

---

## 8. Failure Modes, Fault Tolerance & Recovery

| Failure Scenario | Mitigation & Architectural Defense |
| :--- | :--- |
| **Plugin Infinite Loop or Crash** | Sandboxed in Extism with execution fuel/memory limits. Panics are trapped; plugin resets without affecting the host editor. |
| **Compiler Timeout (Heavy LaTeX Pass)** | Tectonic runs in a dedicated Rayon worker with a 10-second timeout. Cancels safely if packages loop infinitely. |
| **Unsaved Edits on Sudden Power Failure** | All Rope mutations write to an append-only Write-Ahead Log (`.manang/wal.log`) on every keystroke debounce (50ms). Instant crash recovery on startup. |
| **Malformed Typst / LaTeX Syntax** | Tree-sitter error recovery preserves unaffected AST branches; the editor displays the last successfully rendered preview while showing inline diagnostic lenses on the broken line. |
| **Offline Sync Conflicts** | Yjs CRDT mathematically guarantees eventual consistency. Reconnecting clients merge concurrent changes deterministically without losing data. |
