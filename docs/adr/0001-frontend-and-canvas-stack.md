# ADR-0001: Frontend Framework & Vector Canvas Stack

**Status:** Accepted  
**Date:** 2026-10-17  
**Deciders:** Fundaments.work  
**Supersedes:** React 19 + shadcn/ui + Excalidraw (implicit choices in architecture plan v1.0.0)  
**Affects:** `DESIGN.md` Screen 2 & 4, `AGENTS.md` frontend rules, architecture plan §3, §4.1, §8

---

## 1. Context

Architecture plan v1.0.0 specified **React 19 + shadcn/ui** for the frontend and **Excalidraw (MIT)** for the freeform canvas. Both were inherited defaults rather than validated decisions, and each carries a structural problem for Manang.

---

## 2. Decision

1. **The frontend is Svelte 5 (runes) + Vite + Tailwind CSS v4.** No React anywhere in the bundle.
2. **The freeform canvas is an in-house typed SVG scene graph**, using `perfect-freehand` (MIT, 2 KB gzip) for pressure-sensitive stroke geometry.
3. **Figures use Manang's own `.figure.json` scene format** rendered to deterministic SVG/PDF by the Rust core, rather than the `.excalidraw` format.
4. **Plain Svelte, not SvelteKit.** Manang is a local-first desktop application; SvelteKit's server/routing/SSR machinery is dead weight inside a Tauri webview. This matches the official `create-tauri-app` Svelte template.

---

## 3. Why the framework choice does not threaten performance

The performance SLAs in `AGENTS.md` are governed by components that are framework-agnostic:

| Concern | Owner | Framework-dependent? |
| :--- | :--- | :--- |
| Keystroke latency (<8 ms) | CodeMirror 6 `EditorView` | No — CM owns its own DOM root |
| Incremental recompile (<50 ms) | Rust `typst` crate | No |
| Typing buffer (ropey) | Rust | No |
| CRDT merges | Yjs + `y-codemirror.next` | No |
| PDF/SVG preview | `pdfjs-dist` | No |
| Native IPC | `@tauri-apps/api` | No |

The view framework never touches the hot path. This is why the swap is low-risk: no SLA is traded away.

---

## 4. Why Excalidraw had to go

`@excalidraw/excalidraw@0.18.1` declares hard peer dependencies:

```
react:      ^17.0.2 || ^18.2.0 || ^19.0.0
react-dom:  ^17.0.2 || ^18.2.0 || ^19.0.0
```

* Its `exports` map is `[".", "./*", "./index.css"]` — there is **no framework-agnostic entry point**.
* The standalone/vanilla build is **not published** to npm. Probing `dist/dev/index.html`, `dist/prod/index.html`, and `dist/excalidraw-standalone.js` all returned `404`, so an iframe or webview approach would require building Excalidraw from source.
* Every Svelte wrapper found is either a React-island hack or abandoned: `tips-services/svelte-excalidraw` (3 stars), `astrosnat/sveltedraw` ("Attempting to port", 0 stars), `joysofcode/svelte-excalidraw` (last push Nov 2024).
* Weight: **344 KB gzip, 31 transitive dependencies**, plus ~45 KB gzip of React runtime.

Keeping it would have permanently required a React island inside a Svelte app.

---

## 5. Alternatives evaluated

| Option | gzip | Deps | License | Verdict |
| :--- | ---: | ---: | :--- | :--- |
| **Svelte + own SVG scene graph** | **~2 KB** | **0** | MIT | **Accepted** |
| Svelte + Excalidraw React island | 344 KB + 45 KB | 31 + 2 | MIT | Rejected — keeps React alive permanently |
| Svelte + js-draw | 142 KB | 2 | MIT | Rejected — 298 stars, CJS-only (no ESM), scene model not Rust-renderable |
| Svelte + Konva + svelte-konva | 55 KB | 0 | MIT | Rejected — canvas-first, `toDataURL()` is raster, conflicts with vector export; hairlines need pixel-offset hacks |
| Svelte + Fabric.js | 90 KB | 0 | MIT | Rejected — canvas-first, heaviest object model |
| Svelte + rough.js | 9 KB | 4 | MIT | Rejected — no release since Nov 2023, dormant |
| Svelte + tldraw | ~90 KB | many | **Custom, commercial** | Rejected — *not* MIT; watermark/commercial terms, and React-peered. Explicitly banned by `AGENTS.md`. |

**Rejected — stay on React:** Excalidraw is native to React, but it is the *only* thing arguing for React. Keeping a virtual DOM for a shell that renders almost nothing hot is a poor trade, and it forfeits the Rust-renderable scene model.

---

## 6. Consequences

**Positive**
* No React in the bundle; no second framework to version, audit, or secure.
* Figure output is deterministic vector SVG/PDF, rendered headlessly by Rust — satisfying `AGENTS.md`'s zero-copy and reproducibility invariants.
* Native SVG rendering honours the design system exactly (`--radius: 0`, 1px hairlines, no rounded corners) without fighting a canvas abstraction.
* `AGENTS.md`'s "no proprietary canvas SDKs" rule is satisfied by construction rather than by review.

**Negative / accepted trade-offs**
* We must build selection, multi-select, resize handles, and undo ourselves. This is bounded by the deliberately narrow pen tray in `DESIGN.md` §4.1 (freehand, geometry, arrow, math text).
* Breaking change to the figure file format (`.excalidraw` → `.figure.json`). Acceptable: the project has no users yet, and we own both writer and renderer.
* Smaller hiring pool than React. Acceptable for a small internal laboratory tool.

**Neutral**
* `shadcn/ui` is replaced. Where headless primitives are needed, use `bits-ui` (MIT-compatible, actively maintained, Svelte-native) — but prefer hand-rolled primitives, since the design system's zero-radius hairline aesthetic is not what shadcn targets.
* `Allotment` (React split-pane) is not used; split panes are plain flexbox with a drag handle.

---

## 7. Verification

The decision rests on registry and repository metadata retrieved on 2026-10-17:

* npm registry: package versions, `license`, `peerDependencies`, `exports`, publish timestamps
* GitHub API: repository activity, archival status, license SPDX IDs, wrapper popularity
* bundlephobia: minified and gzip bundle sizes
* Tauri v2 docs (`v2.tauri.app/start/create-project/`): Svelte is an officially maintained `create-tauri-app` template
