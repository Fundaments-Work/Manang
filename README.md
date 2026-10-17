# Manang

> *The High-Performance Scientific Authoring & Illustration Suite*  
> **Made by Fundaments.work**

Manang combines **Typst**, **LaTeX (Tectonic)**, **Markdown**, an in-house **vector figure studio**, and a dedicated **CeTZ / TikZ Visual Studio** into a local-first, ultra-fast authoring suite.

---

## Architecture & Documentation

- **[Architecture & Design Plan](docs/plans/2026-10-13-manang-architecture-design.md)** — Core engine design, research benchmarks, concurrency model, and phased roadmap.
- **[System Design Specification (DESIGN.md)](DESIGN.md)** — Domain entities, IPC interfaces, and visual tokens governed by the Fundaments.work design system.
- **[Agent Guidelines & Testing Harness (AGENTS.md)](AGENTS.md)** — Invariants, testing pyramid (`proptest`, `insta`, `cargo-fuzz`, `criterion`), and Do's & Don'ts.
- **[ADR-0001: Frontend & Canvas Stack](docs/adr/0001-frontend-and-canvas-stack.md)** — Why the shell is Svelte 5 and the figure studio is an in-house SVG scene graph.

---

## Development

Requires Rust 1.85+, Node 22+, and pnpm 10. On Linux the Tauri shell needs
`webkit2gtk-4.1`, `gtk3`, `libsoup-3.0`, and `glib-2.0` development packages.

```bash
pnpm install          # install the frontend workspace
pnpm app:dev          # run the desktop shell (Vite + Tauri)
pnpm typecheck        # svelte-check + tsc
pnpm test             # vitest, pure-logic unit suites
pnpm lint             # eslint
pnpm verify:ui        # headless UI verification (needs the dev server running)
```

The UI verification drives the system Chromium over the DevTools protocol
rather than downloading a browser, so it runs offline and against the same
binary used in development. It writes annotated screenshots to
`artifacts/screens/`.

Rust quality gates, per `AGENTS.md`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all
```

### Layout

```
Cargo.toml          Rust workspace (release profile lives here, not in a member)
src-tauri/          Tauri shell: lifecycle and IPC surface only
frontend/           Svelte 5 presentation shell
  src/app.css       Design tokens — the single source of UI truth
  src/lib/editor/   CodeMirror 6 mount, theme, Margin Whispers
  src/lib/ink/      Figure scene graph and geometry (pure, unit tested)
  src/lib/commands/ Command registry and fuzzy matcher
  src/lib/state/    Rune-based view state, kept close to the leaves
scripts/            Headless UI verification
docs/plans/         Architecture and roadmap
docs/adr/           Architecture decision records
```

---

### License & Attribution
Part of the [Fundaments.work](https://fundaments.work) laboratory tools ecosystem.
