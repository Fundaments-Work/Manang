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

### 3.1. Layout Philosophy: Ergonomic 3-Column Split
* **Left Column (Collapsible, 240px default):**
  - Project file tree with clean icons.
  - Document outline / symbol hierarchy (sections, subsections, figures, tables).
  - Bibliography references panel.
* **Center Column (Flexible, ~50% viewport):**
  - **Code Editor (CodeMirror 6):** Line numbers, folding, inline error badges, and breadcrumb bar.
  - **Canvas Tab / Modal:** Instantly switch to embedded Excalidraw or the CeTZ/TikZ visual sandbox.
* **Right Column (Collapsible, ~50% viewport):**
  - **Live Preview:** High-framerate PDF.js / SVG viewer.
  - Synchronized scrolling and **SyncTeX click-to-jump** navigation.
  - Sidekick Agent drawer (Prompt-to-Doc co-author, citation checker).

### 3.2. Visual Styling & Color Tokens
* **Dark Theme (Default):** Deep Slate/Obsidian palette (`#090D16` background, `#131B2E` panels, `#1E293B` borders). High-contrast syntax highlighting optimized for mathematical equations.
* **Light Theme:** Natural Paper/Ivory palette (`#FDFCF9` background, `#F4F1EA` panels, `#E5E0D8` borders) simulating high-quality academic paper stock.
* **Typography:**
  - UI Prose: `Inter`, `Geist Sans`, or system sans-serif.
  - Code Editor: `JetBrains Mono` or `Fira Code` (with ligature support for `->`, `=>`, `!=`, `<=`).
  - Document Typesetting: `New Computer Modern` (LaTeX), `Linux Libertine` / `Libertinus` (Typst).

### 3.3. Command Palette & Navigation
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
