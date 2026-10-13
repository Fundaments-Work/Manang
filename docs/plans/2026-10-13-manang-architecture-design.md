# Manang: Architecture & System Design Specification

**Document Version:** 1.0.0  
**Date:** 2026-10-13  
**Status:** Validated  
**Project:** Manang (The High-Performance Scientific Authoring & Illustration Suite)

---

## 1. Executive Summary & Vision

**Manang** is an ultra-fast, local-first, extensible scientific document and slide authoring platform. It bridges the speed of **Typst**, the universality of **LaTeX** (powered by an embedded Rust **Tectonic** engine), and the ubiquity of **Markdown**, integrated with a unified **Visual Studio** featuring **Excalidraw** (100% MIT) and a dedicated **CeTZ / TikZ visual development environment**.

Built around a **Rust-first, WASM plugin-first architecture**, Manang relegates heavy computing, compilation, text buffer operations, and file I/O to a multi-threaded Rust core. The frontend remains a thin, high-framerate presentation shell running in Tauri v2. Additionally, an autonomous **Scientific Generative Agent** provides prompt-to-document synthesis, prompt-to-slides generation, citation verification against scholarly indexes (OpenAlex, Crossref, arXiv), and compiler self-healing.

---

## 2. System Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                        THIN PRESENTATION LAYER                         │
│  [Tauri v2 Webview Shell - 60/120 FPS Rendering & Event Dispatch]      │
│  ├── CodeMirror 6 (Lightweight view, virtualized lines, SyncTeX jump)  │
│  ├── Excalidraw Engine (MIT infinite canvas, freeform sketching)       │
│  ├── CeTZ / TikZ Visualizer (Live isolated figure sandbox & gallery)   │
│  ├── PDF & SVG Viewers (PDF.js + vector streaming)                     │
│  └── Agent UI Panel (Prompt-to-doc co-pilot, inline diff previews)    │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Zero-Copy Binary IPC (ArrayBuffers)
┌───────────────────────────────────▼────────────────────────────────────┐
│                    MANANG RUST CORE (NATIVE DAEMON)                    │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │   Memory Engine: Ropey Buffer + Tree-sitter Incremental AST      │  │
│  │   • O(log N) operations on 100,000+ line documents               │  │
│  │   • Lock-free synchronization across threads                     │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  ┌──────────────────────────────┐    ┌──────────────────────────────┐  │
│  │   Rayon Thread Pool (CPU)    │    │    Tokio Runtime (Async I/O) │  │
│  │  • Typst In-Memory Crate     │    │  • Multi-Provider LLM Client │  │
│  │  • Tectonic XeTeX Engine     │    │  • Scholarly API Gateway     │  │
│  │  • Excalidraw Headless Export│    │  • File Watcher & Git (gix)  │  │
│  │  • Hayagriva BibTeX / CSL    │    │  • Yjs Binary WebSocket/P2P  │  │
│  └──────────────────────────────┘    └──────────────────────────────┘  │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │   Extism WASM Plugin Runtime (Wasmtime Sandboxed Execution)      │  │
│  │   • Linters (Vale, ChkTeX)   • Exporters (Pandoc, Quarto, Marp)  │  │
│  │   • Custom Canvas Shapes     • Dynamic Agent Tools (MCP Host)    │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Technology Stack & Selection Rationale

| Layer | Technology | Selection Rationale & Research Basis |
| :--- | :--- | :--- |
| **Desktop Shell** | **Tauri v2 + Rust** | ~35–50 MB idle RAM footprint (vs. Electron's 400MB+). Direct zero-overhead access to native OS threads, file system, and GPU. |
| **Frontend UI** | **React 19 + Vite + Tailwind CSS + shadcn/ui** | Clean integration with Excalidraw React core, modular split-pane layouts (Allotment), and modern accessible primitives. |
| **Editor Core** | **CodeMirror 6** | Ultra-lightweight (~150 KB vs. Monaco's 5 MB+), instant startup (<100ms), functional immutable state architecture preventing DOM thrashing during real-time CRDT sync. |
| **Typst Compiler** | **`typst` crate (Pure Rust)** | Sub-50ms incremental compilation through memoized syntax trees. Standalone binary with zero system dependencies. |
| **LaTeX Compiler** | **`tectonic` crate + TeX Live Fallback** | Self-contained XeTeX engine in Rust. Auto-fetches CTAN packages on demand into a local cache without requiring 5GB+ TeX Live installs. Transparent fallback to system TeX when `--shell-escape` is needed. |
| **Freehand Canvas** | **Excalidraw (MIT License)** | 100% free and open-source forever. No commercial licensing traps, no client watermarks. Headless export to vector SVG and PDF. |
| **Mathematical Graphics**| **CeTZ (Typst) & TikZ (LaTeX)** | Mathematically precise code-based vector graphics integrated with a live preview sandbox and visual snippet gallery. |
| **Buffer Structure**| **`ropey` (Rust B-Tree Rope)** | Handles multi-megabyte papers with $O(\log N)$ inserts and deletions. Avoids JS string reallocation bottlenecks. |
| **Citations** | **`hayagriva` (Pure Rust)** | Typst's native CSL and BibLaTeX engine. Extremely fast sorting, formatting, and bibliography resolution. |
| **Plugin Engine** | **Extism (powered by Wasmtime)** | Safe WebAssembly sandbox with capability-based permissions. Supports plugins written in Rust, TypeScript, Go, Zig, and Python. |
| **Agent Runtime** | **Tokio Async + MCP Host** | Model-agnostic LLM gateway (Claude, GPT, Gemini, Ollama) equipped with Model Context Protocol (MCP) tool bindings. |
| **Real-Time Collab**| **Yjs Binary Protocol over WebSockets** | Industry-standard CRDT providing conflict-free real-time co-authoring, offline-first resilience, and deterministic merges. |
| **Version Control** | **`gix` (Pure Rust Git)** | Local-first version history, branch management, and remote syncing without shell subprocess dependencies. |

---

## 4. Visual Environment & Asset Pipeline

Manang supports a **dual visual workflow**:

### 4.1. Excalidraw Infinite Canvas (Freeform)
1. Creating a diagram creates a `.excalidraw` file inside the project's `figures/` directory.
2. The canvas is rendered in a dedicated pane or floating modal.
3. On save, the headless Rust pipeline renders deterministic vector `.svg` and `.pdf` files.
4. The editor auto-inserts:
   - **Typst:** `#image("figures/diagram.svg", width: 80%)`
   - **LaTeX:** `\includegraphics[width=0.8\textwidth]{figures/diagram.pdf}`
   - **Markdown:** `![diagram](figures/diagram.svg)`
5. Clicking the rendered figure in the PDF or markdown preview automatically re-opens the editable Excalidraw canvas.

### 4.2. CeTZ & TikZ Visual Studio (Mathematical Vectors)
1. **Isolated Preview Canvas:** Live sandbox for editing CeTZ (Typst) and TikZ (LaTeX) code with instant feedback.
2. **Visual Snippet Library:** Drag-and-drop / one-click templates for coordinate plots, state machines, tree hierarchies, and neural network diagrams.
3. **Agent Synthesis:** Users can prompt: *"Generate a 3-layer feedforward neural network diagram in CeTZ"*, review the visual render in the sandbox, and insert the code directly into the document.

---

## 5. The Generative Scientific Agent & Self-Healing Loop

The Agentic Engine acts as an active technical author and reviewer:

```
[ Natural Language Prompt ] ──► [ Planning & Layout Agent ]
                                              │
                     ┌────────────────────────┴────────────────────────┐
                     ▼                                                 ▼
        [ Document Synthesis ]                                [ Slide Deck Synthesis ]
     (Papers, Reports, Resumes)                             (Typst Touying, Polylux, Beamer)
                     │                                                 │
                     └────────────────────────┬────────────────────────┘
                                              ▼
                                 [ Code Generation Engine ]
                                              │
                                              ▼
                             ┌─────────────────────────────────┐
                             │   Rust Compiler Evaluation      │
                             │   (Typst / Tectonic Crate)      │
                             └───────────────┬─────────────────┘
                                             │
                        ┌────────────────────┴────────────────────┐
                        │ Clean Build                             │ Error Detected
                        ▼                                         ▼
            [ Stream PDF / Slides ]                    [ Diagnostic Diagnosis ]
                                                                  │
                                                                  ▼
                                                       [ Auto-Patch Diff ]
                                                                  │
                                                                  └──► (Re-evaluate)
```

### 5.1. Capabilities:
1. **Prompt-to-Document:** Generates complete, styled articles and technical reports conforming to target templates (IEEE, ACM, Nature, ArXiv).
2. **Prompt-to-Slides:** Builds interactive presentation decks using modern Typst frameworks (**Touying**, **Polylux**) and LaTeX **Beamer**.
3. **Citation & Fact Verification:**
   - Extracts citations (`\cite{...}`, `@ref`).
   - Resolves DOIs and titles against OpenAlex, Crossref, and arXiv APIs.
   - Compares the author's statements against the cited paper's abstract to detect hallucinations or unsupported assertions.
4. **Autonomous Compiler Self-Healing:**
   - Intercepts compiler errors from Typst or Tectonic.
   - Formulates targeted AST patches and presents a 1-click diff to the user.

---

## 6. WASM Plugin Architecture & Sandboxing

All extensions run within isolated WebAssembly virtual machines:

* **Host Integration:** Managed via **Extism** with memory fuel limits to prevent runaway loops.
* **Manifest Permissions:**
  ```toml
  [plugin]
  name = "marp-slides"
  version = "1.0.0"
  description = "Marp slide generation plugin for Markdown"

  [permissions]
  filesystem = ["read:templates"]
  network = []
  buffer = ["read:active", "write:active"]
  ```
* **Hook Points:**
  - `on_compile_pre`: Pre-process document source (e.g., custom macro expansions).
  - `on_compile_post`: Post-process generated outputs (e.g., EPUB bundle generation).
  - `on_lint`: Deliver diagnostics into CodeMirror inline error lenses.
  - `register_agent_tool`: Expose custom MCP tools to the AI agent.

---

## 7. Storage, Directory Structure & Git Collaboration

```
my-paper/
├── manang.toml          # Project configuration, engine target, template bindings
├── main.typ             # Document root (or main.tex / index.md)
├── figures/             # Illustration assets
│   ├── arch.excalidraw  # Editable Excalidraw vector scene
│   ├── arch.svg         # Auto-generated vector SVG (Typst / Web)
│   └── arch.pdf         # Auto-generated vector PDF (LaTeX)
├── references.bib       # BibTeX bibliography file
└── .manang/             # Ephemeral local cache & CRDT log (git-ignored)
    ├── crdt/            # Append-only Yjs delta journal
    ├── tectonic_cache/  # Downloaded CTAN bundles
    └── wasm_cache/      # Compiled plugin modules
```

* **Local-First Plaintext:** Files remain clean UTF-8 text directly accessible by standard command-line tools.
* **Git Transparency:** Native version control powered by the `gix` Rust crate.
* **Real-Time CRDT Sync:** Yjs binary updates sent over WebSockets or WebRTC for seamless co-authoring without cloud lock-in.

---

## 8. Implementation Phases & Milestones

1. **Phase 1: Rust Core & Dual Compilers**
   - Tauri v2 + React 19 workspace scaffolding.
   - In-memory `typst` and `tectonic` crate integration.
   - CodeMirror 6 with bidirectional SyncTeX and PDF.js viewer.
2. **Phase 2: Visual Studio (Excalidraw & CeTZ/TikZ)**
   - Excalidraw React embedding with headless SVG/PDF compiler.
   - CeTZ & TikZ live preview sandbox and scientific template palette.
3. **Phase 3: WASM Plugin Engine**
   - Extism runtime integration with capability manifests.
   - Initial plugins: Vale prose linter and Typstyle formatter.
4. **Phase 4: Scientific Generative Agent & MCP Host**
   - Multi-provider LLM gateway (Claude, OpenAI, Gemini, Ollama).
   - Prompt-to-document and slide generation pipelines.
   - Scholarly citation verifier and compiler self-healing loop.
5. **Phase 5: Collaborative CRDT & Git Engine**
   - Yjs binary synchronization over WebSockets.
   - Embedded `gix` Git client for local and remote repositories.
