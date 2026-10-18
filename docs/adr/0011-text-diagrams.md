# ADR 0011: Text diagrams - Mermaid, Pikchr and ASCII to deterministic SVG

Status: accepted (2026-10-18)
Scope: `crates/manang-diagram`

## Context

Scientific documents need diagrams written as text next to the prose:
flowcharts, sequence and state diagrams, Gantt charts, simple pictures. The
rendered figure must be identical on every machine (it is committed and
diffed), drawable by usvg so Typst and PDF export can embed it, and themed
with the Fundaments.work tokens. `AGENTS.md` requires the logic in Rust, no
statically bundled C libraries in the app binary, and a core that compiles to
`wasm32-unknown-unknown` for the planned web version.

## Decisions

### Mermaid: `merman` =0.9.0, without ELK

`merman` (MIT OR Apache-2.0) is a headless Rust port of Mermaid 12.1 with
parsing, layout and SVG emission, a deterministic text measurer and an
operation model with no ambient clock or randomness (Gantt's "today" is the
Unix epoch unless set). It is pinned exactly because its SVG bytes are
snapshot-tested.

* Features: `default-features = false`, `complete-svg` (SVG, Cytoscape
  layout, RaTeX math) and every `diagram-*` family of 0.9.0, listed one by
  one so a new upstream family is an explicit decision.
* **No ELK.** merman's ELK layout port is EPL-2.0, which we do not accept
  for now. `cargo tree -p manang-diagram` must not show it.
* Output goes through merman's `ResvgSafe` pipeline with `htmlLabels: false`:
  labels are SVG `<text>`, never `<foreignObject>`, which usvg and Typst
  cannot draw.
* Math (`$$...$$`) uses RaTeX with its bundled KaTeX fonts. Formulas with
  CJK or emoji are declined because RaTeX would scan system fonts for them.
* Text is measured by merman's deterministic measurer. The app ships DM Sans
  and DM Mono only as WOFF/WOFF2 through `@fontsource`; there are no TTF/OTF
  bytes in the repository for a `ttf-parser` measurer, so labels are laid
  out with merman's metrics and drawn in `DM Sans, sans-serif`.

### Pikchr: `pikru` =1.0.1

Pure-Rust port of the pikchr C reference (MIT OR Apache-2.0). Stable 1.0.1
is preferred over the 2.0 release candidate. Syntax errors are located by
running its pest grammar directly, which reports byte offsets.

### ASCII diagrams: `svgbob` =0.7.2

Apache-2.0. 0.7.3 to 0.7.6 depend on `sauron` 0.61, which pins
`futures = "=0.3.30"` and cannot resolve next to Wasmtime's `futures ^0.3.31`;
0.7.2 (`sauron` 0.60) has no such pin. Revisit when svgbob drops the pin.

### MSRV 1.95

merman declares `rust-version = 1.95`; the workspace MSRV moves from 1.92 to
1.95.

### Theming

Three themes: `dark` and `light` (transparent background, workspace tokens
from `src/app.css`, the light accent from ADR 0002) and `print` (white page,
ink lines) for committed files and PDF. Mermaid is themed through
`theme: base` and `themeVariables`, so colours are plain values in the SVG
(usvg and Typst do not resolve CSS variables). Rules that no engine option
expresses are enforced on the XML with `quick-xml`, never with regular
expressions on markup: rectangles get `rx="0"`, `<filter>` elements and
references are removed (no shadows), `var(--…)` declarations are dropped, and
every colour that is not a design token is mapped to the token of nearest
perceptual lightness. Mermaid's hard-coded literals and Pikchr's colours
assume a light page, so their lightness is inverted on the dark theme. The
accent is opt-in: a flowchart, state or class diagram that uses the class
`accent` without defining it gets `classDef accent` appended (appending keeps
line numbers), and `diagram.check` warns when more than one element uses it.

## Alternatives rejected

* **mermaid.js in the webview**: about 5.5 MB minified, needs a DOM, measures
  text with the browser's fonts (not deterministic), and cannot run during
  headless PDF export.
* **mermaid-cli**: drives a headless Chromium.
* **mermaid-rs-renderer**: early, and scans system fonts.
* **flowmaid**: GPL-3.0.
* **PlantUML**: Java.
* **D2**: Go, with only a partial WASM port.

## Deferred

* **Graphviz** through the `diagraph` Typst package, after an EPL licence
  check.
* A vendored `@preview/merman` Typst package for inline `#mermaid` in Typst.
* **ELK** layout, if its licence becomes acceptable.

## How documents will use diagrams (later package)

Document integration is not part of this crate. The plan:

* Typst and Markdown include a sibling `.svg` rendered next to each `.mmd`
  (or `.pikchr`, `.bob`); LaTeX includes a sibling `.pdf`.
* Inline Markdown fences render to `.manang/cache/diagrams/<key>.svg`, where
  `<key>` is `diagram.key`: a BLAKE3 hash of the language, engine and
  version, this crate's output revision, the theme and the source.

## Command contract

| Command | Input | Output |
| :--- | :--- | :--- |
| `diagram.languages` | `{}` | `{ languages: [{ id, name, extensions, fences, engine }] }` |
| `diagram.check` | `{ language, source }` | `{ diagnostics: [{ severity, message, line, column, endLine, endColumn, code? }] }` |
| `diagram.render` | `{ language, source, theme }` | binary SVG (UTF-8) |
| `diagram.key` | `{ language, source, theme }` | `{ key }` (64 hex digits) |

Lines and columns are 1-based, columns counted in Unicode scalar values, end
exclusive, as in the Typst plugin's diagnostics. Render failures are
`[diagram_error] line L:C: <message>`, `[unknown_language] …`,
`[invalid_argument] …` or `[render_error] …`. All commands are pure and need
no permission.

## Consequences

* About 110 new crates in `Cargo.lock`, all pure Rust; no `-sys` crate builds
  C. `svgbob` brings `wasm-bindgen`/`web-sys` through `sauron`; they compile
  to nothing on native targets.
* The crate checks for `wasm32-unknown-unknown`.
* Snapshot tests hold the exact SVG bytes of one fixture per family in the
  dark and print themes; any upstream change to layout or output shows up as
  a diff and needs a deliberate review when merman is bumped.
