# Manang

> _The High-Performance Scientific Authoring & Illustration Suite_  
> **Made by Fundaments.work**

Manang combines **Typst**, **LaTeX (Tectonic)**, **Markdown**, an in-house **vector figure studio**, and a dedicated **CeTZ / TikZ Visual Studio** into a local-first, ultra-fast authoring suite.

---

## Architecture & Documentation

- **[Architecture & Design Plan](docs/plans/2026-10-13-manang-architecture-design.md)** — Core engine design, research benchmarks, concurrency model, and phased roadmap.
- **[System Design Specification (DESIGN.md)](DESIGN.md)** — Domain entities, IPC interfaces, and visual tokens governed by the Fundaments.work design system.
- **[Agent Guidelines & Testing Harness (AGENTS.md)](AGENTS.md)** — Invariants, testing pyramid (`proptest`, `insta`, `cargo-fuzz`, `criterion`), and Do's & Don'ts.
- **[ADR-0001: Frontend & Canvas Stack](docs/adr/0001-frontend-and-canvas-stack.md)** — Why the shell is Svelte 5 and the figure studio is an in-house SVG scene graph.

---

## User guide

A guide for authors in [`docs/user/`](docs/user/index.md):

- **[Getting started](docs/user/getting-started.md)** — Running from source, Home view, creating projects, and window layout.
- **[Writing](docs/user/writing.md)** — Typst, LaTeX, and Markdown editing, spell check, formatting, snippets, and word counts.
- **[Preview and PDF](docs/user/preview-and-pdf.md)** — Preview layouts, live recompile, SyncTeX jumps, PDF export, and TeX Live setup.
- **[Figures and diagrams](docs/user/figures-and-diagrams.md)** — Vector figure editor, text diagrams (Mermaid, Pikchr, ASCII), and document includes.
- **[References](docs/user/references.md)** — Bibliographies, citation styles, inserting citations, and online verification.
- **[Projects and files](docs/user/projects-and-files.md)** — File tree, openers, system apps, and Git source control.
- **[Settings and shortcuts](docs/user/settings-and-shortcuts.md)** — User vs project scope, preferences, and keyboard shortcuts.
- **[Menus and shortcuts](docs/user/menus-and-shortcuts.md)** — Menu bar hierarchy, keyboard mnemonics, and surface keybindings.
- **[Plugins](docs/user/plugins.md)** — Built-in and sandboxed WebAssembly plugins, permissions, and status indicators.
- **[Troubleshooting](docs/user/troubleshooting.md)** — Log file locations, developer tools, and common problem fixes.

---

## Development

Requires Rust 1.95+, Node 22+, and pnpm 10. On Linux the Tauri shell needs
`webkit2gtk-4.1`, `gtk3`, `libsoup-3.0`, and `glib-2.0` development packages.

```bash
pnpm install

pnpm app:dev          # run the desktop app (Vite + Tauri, native window)
pnpm dev              # run only the frontend, in a browser at :1420
pnpm app:build        # bundle the desktop app for distribution
```

The repository uses the canonical Tauri layout: the frontend lives at the root
and `src-tauri/` is the only application subfolder, so every Tauri command,
config path and CI recipe works without overrides.

| Command | Purpose |
| :--- | :--- |
| `pnpm app:dev` | The real desktop app — native window, Rust backend, HMR |
| `pnpm dev` | Frontend only, in a browser. Useful for CSS work |
| `pnpm app:build` | Production bundle, under `target/release/` |
| `pnpm typecheck` | `svelte-check` plus `tsc` |
| `pnpm test` | Vitest, pure-logic unit suites |
| `pnpm lint` | ESLint |
| `pnpm format` | Prettier, code only — the specs are never reformatted |
| `pnpm verify:ui` | Headless UI verification against a running dev server |

`pnpm verify:ui` drives the system Chromium over the DevTools protocol rather
than downloading a browser, so it runs offline and against the same binary used
in development. It writes screenshots to `artifacts/screens/`.

Rust quality gates, per `AGENTS.md`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all
```

### Layout

```
Cargo.toml          Rust workspace (release profile lives here, not in a member)
index.html          Frontend entry document
package.json        Frontend and Tauri CLI scripts
src/                Svelte 5 presentation shell
  app.css           Design tokens — the single source of UI truth
  lib/editor/       CodeMirror 6 mount, theme, Margin Whispers
  lib/ink/          Figure scene graph and geometry (pure, unit tested)
  lib/commands/     Command registry and fuzzy matcher
  lib/state/        Rune-based view state, kept close to the leaves
src-tauri/          Tauri shell: lifecycle and IPC surface only
scripts/            Headless UI verification
docs/plans/         Architecture and roadmap
docs/adr/           Architecture decision records
```

### Keyboard

| Shortcut | Action |
| :--- | :--- |
| `Ctrl/Cmd + K`, or `/` | Command palette |
| `Ctrl/Cmd + P` | Library shelf |
| `Ctrl/Cmd + \` | Zen mode |
| `F5` | Presentation mode |
| `←` `→` | Navigate slides while the studio is open |
| `Esc` | Dismiss the topmost overlay |

Screens are also addressable by hash, which is what the verification harness
drives: `#library`, `#draw`, `#shelf`, `#slides/2`, `#zen`, `#palette/cit`.

---

### License & Attribution

Part of the [Fundaments.work](https://fundaments.work) laboratory tools ecosystem.