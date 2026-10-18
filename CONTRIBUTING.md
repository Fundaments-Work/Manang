# Contributing to Manang

Thank you for your interest in contributing to Manang! This guide covers how to set up your local development environment, run quality gates, adhere to architectural invariants, and navigate the codebase.

Before contributing, please review [AGENTS.md](AGENTS.md), which serves as the binding operational and architectural specification for this repository.

---

## 1. Prerequisites & Toolchain

Ensure your system has the following toolchains installed:

- **Rust:** Version `1.95` or newer (edition 2024). Verified via `rustc --version`.
- **Node.js:** Version `22` or newer (`node --version`).
- **pnpm:** Version `10.18` or newer (`pnpm --version`).
- **Tauri Prerequisites:** Standard system libraries for Tauri 2 (e.g. `libwebkit2gtk-4.1-dev` and `libappindicator3-dev` on Linux).

---

## 2. Getting Started & Running Locally

### 2.1. Clone and Install Dependencies
```bash
git clone https://github.com/Fundaments-Work/Manang.git
cd Manang
pnpm install --frozen-lockfile
```

### 2.2. Port Conventions
- **Port `1420`:** Reserved exclusively for the primary desktop app dev server (`pnpm app:dev`).
- **Ports `1431`–`1445`:** Reserved for parallel automated agent sessions and headless test servers.
- Check if port 1420 is listening:
  ```bash
  lsof -iTCP:1420 -sTCP:LISTEN
  ```

### 2.3. Run the Desktop App
Launch the full Tauri desktop application connected to the Vite frontend:
```bash
pnpm app:dev
```

### 2.4. Run the Web Dev Shell (Mock Kernel)
For rapid frontend iteration without compiling native Rust binaries, run the Vite browser shell (which boots with full in-memory mock plugins):
```bash
pnpm dev
```

---

## 3. Architecture Overview & Codebase Map

All compute-intensive operations (parsing, AST indexing, compilation, sandboxing) live in Rust under `crates/`. The frontend under `src/` is strictly a thin Svelte 5 presentation and event-dispatching shell.

### 3.1. Crates Map (`crates/`)
| Crate | Responsibility |
| :--- | :--- |
| `src-tauri` | Desktop application entry point, window lifecycle, system tray, and native shell registration |
| `crates/manang-kernel` | Extensible plugin kernel, command registry, message bus, and capability verification |
| `crates/manang-wasm-host` | WebAssembly component-model runtime (Wasmtime) executing sandboxed third-party plugins |
| `crates/manang-plugins` | Plugin management lifecycle (install, verify consent, enable/disable, uninstall) |
| `crates/manang-editor` | Multi-document text buffer service, transactional edits, and coordinate conversions |
| `crates/manang-typst` | High-performance Typst compiler, streaming SVG preview, and SyncTeX jump mapping |
| `crates/manang-latex` | LaTeX compilation pipeline using system TeX distributions or optional worker sidecar |
| `crates/manang-project` | Project registry, scaffolding, templates loader, and recoverable trash lifecycle |
| `crates/manang-buffer` | Rope-backed text document buffer (`DocumentBuffer`) with undo/redo transactions |
| `crates/manang-figure` | In-house vector figure engine (scene v2), freehand ink, geometric shapes, and SVG export |
| `crates/manang-settings` | Hierarchical configuration (user vs. project scope), schemas, and keybinding conflicts |
| `crates/manang-git` | Embedded Git repository operations (status, diffs, commits, branches, sync) |
| `crates/manang-search` | Fast fuzzy file search, regex/literal content search, and atomic replace preview/apply |
| `crates/manang-writing` | Scientific writing assistant, spell checker, SCOWL dictionaries, and word counts |
| `crates/manang-markdown` | CommonMark parser, XSS sanitization, math preservation, outline, and Typst conversion |
| `crates/manang-citations` | Citation indexing, BibTeX/CSL-JSON parsing, APA/IEEE formatting, and online verification |
| `crates/manang-diagram` | Text diagram compiler for Mermaid (`.mmd`), Pikchr, and ASCII art (`svgbob`) |
| `crates/manang-agent-core` | AI agent orchestration foundation, risk-class approvals, and tool interfaces |
| `crates/manang-agent-providers` | LLM provider adapters for Anthropic and OpenAI-compatible endpoints |
| `crates/manang-keystore` | Operating system keychain integration for local API key and secret storage |
| `crates/manang-agent-rig` | Rig-backed provider adapter layer evaluating streaming and policy parity |

### 3.2. Frontend Map (`src/lib/`)
| Directory | Responsibility |
| :--- | :--- |
| `src/lib/commands` | Central workbench command definitions, action handlers, and shortcut registrations |
| `src/lib/components` | Primary Svelte 5 desktop workbench components (TopBar, SideBar, EditorPane, PreviewPane) |
| `src/lib/components/figure` | Figure studio interactive canvas, pen tray, tool selectors, and property drawer |
| `src/lib/components/home` | Home view, project list cards, New Project dialog, Template cards, and Trash section |
| `src/lib/components/menu` | Application menu bar (File, Edit, View, Go, Build, Help) with keyboard navigation |
| `src/lib/components/panels` | Sidebar view panels (Files, Search, Source Control, References, Outline, Problems, Output) |
| `src/lib/components/plugins` | Plugins view, details modal, permission audits, and consent dialogs |
| `src/lib/components/settings` | Settings dialog views (General settings form, Keyboard shortcuts table, About/Licenses) |
| `src/lib/components/ui` | Atomic reusable UI primitives (buttons, badges, inputs, modals) adhering to Fundaments design |
| `src/lib/editor` | CodeMirror 6 text editor setup, language extensions (Typst, LaTeX, Markdown, BibTeX), and syntax themes |
| `src/lib/editor/writing` | Formatting tools, spell check linter, personal dictionary decorations, and word counts |
| `src/lib/figure` | Client-side figure state models, stroke geometry, shape snapping, and SVG serialization |
| `src/lib/kernel` | Typed Tauri IPC bridges, event bus, and browser dev-mode mock plugins (`mockGit`, `mockSearch`, etc.) |
| `src/lib/state` | Svelte 5 reactive global workbench states (`workspace`, `documents`, `home`, `figures`, `tabs`, `layout`) |
| `src/lib/theme` | WCAG 2.1 contrast calibration, dark/light theme tokens, and Fundaments brand invariant checks |
| `src/lib/util` | Platform utilities (OS detection, clipboard helpers, byte size and relative time formatters) |
| `src/lib/workbench` | Pure data transformation models and algorithms (search filtering, git grouping, outline parsing) |
| `src/lib/workbench/pdf` | Embedded PDF viewer state, find bar, outline extraction, and SyncTeX jump coordinators |

---

## 4. The Plugin-Per-Feature Convention

Every feature in Manang is implemented as a plugin crate under `crates/manang-<name>`.

1. **Manifest (`manifest.toml`):** Every plugin defines a manifest stating its tier (`native` or `wasm`), name, and explicitly declared capabilities (`filesystem`, `network`, `buffer`).
2. **Export API:** Exposes `manifest()` and `plugin(..)` implementing `manang_kernel::Plugin`.
3. **Registration:** Registered into the central kernel in `src-tauri/src/lib.rs`.
4. **Command Namespace:** Commands must use `<plugin>.<command>` (e.g. `figure.save`, `citations.verify`).
5. **Kernel Integration Tests:** Every command must be exercised against an in-memory `manang_kernel::Kernel` in `crates/manang-<name>/tests/kernel.rs`.

---

## 5. Quality Gates & Verification

Before submitting any commit or pull request, run the following verification steps:

### 5.1. Rust Code Formatting & Lints
```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
```
*Note: Manang enforces a zero-warnings policy. Never use `#[allow(clippy::...)]` without documented justification.*

### 5.2. Rust Tests
Run crate-specific tests while developing:
```bash
cargo test -p <crate> --locked
```
Or run the full suite:
```bash
cargo test --all -- --nocapture
```

### 5.3. Banned Dependency Check
To prevent runtime crashes on Linux and macOS, never link static system libraries into the main binary:
```bash
cargo tree -p manang | grep -iE "tectonic|harfbuzz|freetype-sys|icu4c"
```
*(This output must remain empty).*

### 5.4. Frontend Type Checks & Unit Tests
```bash
pnpm run typecheck
pnpm run lint
pnpm test
```

### 5.5. Headless Visual Verification
Run the end-to-end CDP headless suite:
```bash
pnpm run verify:ui
```

---

## 6. Git Hygiene & Branch Rules

- **Never commit directly to `main`.** All work happens on isolated topic branches: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, `agent/<topic>`, or `antigravity/<topic>`.
- **Conventional Commits:** Follow the standard format:
  - `feat(typst): add streaming svg recompile`
  - `fix(figure): prevent anchor snap jitter on resize`
  - `docs(beta): update tester walk-through tour`
  - `test(citations): add mock verification test for arxiv doi`
- **Memory Safety:** When compiling Rust on resource-constrained development machines, use:
  ```bash
  export CARGO_BUILD_JOBS=4
  export CARGO_TARGET_DIR=/home/sapiens/Development/Manang/target
  ```
  Target specific crates (`-p <crate>`), avoid full workspace rebuilds, and never leave detached dev servers or fuzzer processes running.
