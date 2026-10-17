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

### License & Attribution
Part of the [Fundaments.work](https://fundaments.work) laboratory tools ecosystem.
