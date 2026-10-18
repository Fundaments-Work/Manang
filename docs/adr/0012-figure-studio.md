# ADR 0012: Figure studio - a diagramming canvas that people and agents both drive

Status: accepted (2026-10-18)
Scope: `crates/manang-figure`, `src/lib/figure/*`, `FigureEditor.svelte`,
the kernel's agent-tool registry (`crates/manang-kernel`), and one new
command in `crates/manang-diagram` (`diagram.layout`).

## Context

The figure plugin started as a sketch pad: freehand strokes, rectangles,
ellipses, unbound arrows and formulas, one scene per `.figure.json` with an
SVG written beside it. The product owner wants it to become Manang's
distinctive feature: an Excalidraw/tldraw-class diagramming canvas that also
understands Mermaid and that AI agents (and other plugins) can drive through
declared tools, without touching the canvas.

`AGENTS.md` fixes the frame: logic in Rust, the frontend a thin shell, no
proprietary or React-bound canvas SDK (ADR 0001), the core compiling to
`wasm32-unknown-unknown`, documents as human-readable files, the
Fundaments.work design for UI chrome.

## Research

What the reference tools offer, from their public documentation and
behaviour (tldraw is source-available under the tldraw licence, *not* open
source: we read its docs and used it, we copied nothing).

| Capability | Excalidraw (MIT) | tldraw (source-available) | draw.io (Apache-2.0) |
| :--- | :--- | :--- | :--- |
| Tools | select, hand, rectangle, diamond, ellipse, arrow, line, freedraw, text, image, eraser, laser, frame, embeddable | select, hand, draw, eraser, arrow, text, note, geo (rect, ellipse, diamond, star, cloud …), line, frame, highlight, laser | shape libraries (hundreds), connectors, text, containers, swimlanes |
| Arrow binding | arrows bind to a shape with a *focus* and *gap*; re-route when the shape moves; elbow arrows (2024) route orthogonally around shapes | arrows bind with a normalised anchor inside the target ("precise" vs "imprecise"); bend handle; elbow arrows | connectors with fixed or floating ports, orthogonal/curved/straight edge styles, waypoints |
| Labels | text bound to a container (`containerId`), arrow labels | label inside geo shapes and on arrows | every cell has a label |
| Styling | stroke colour, background, fill style (hachure, cross-hatch, solid), stroke width, stroke style, sloppiness (roughness 0-2), edges (sharp/round), font family, size, alignment, opacity | colour (palette of 12), fill (none, semi, solid, pattern), dash, size, font, alignment | arbitrary CSS-like style string per cell |
| Arrangement | multi-select, marquee, group (nested `groupIds`), z-order, align, distribute, flip, lock, snapping (objects and grid) | the same plus smart guides on drag and resize | align, distribute, arrange, layout algorithms (tree, organic, …) |
| Canvas | infinite canvas, zoom/pan, zoom to fit / selection | the same | paged or infinite |
| Libraries | reusable element libraries (`.excalidrawlib`) | none built in | built-in shape libraries |
| Export | PNG, SVG (optionally embedding the scene), clipboard, with or without background, dark mode, scale; frames as export regions | PNG, SVG, JSON; frames | PNG, SVG, PDF, XML, HTML |
| Mermaid | "Mermaid to Excalidraw": flowcharts become native shapes and bound arrows, other diagram types become an image | none | Mermaid insert as image or as native cells |
| Storage | JSON: `elements[]` with `type`, `x`, `y`, `width`, `height`, `angle`, `strokeColor`, `backgroundColor`, `fillStyle`, `strokeWidth`, `strokeStyle`, `roughness`, `opacity`, `groupIds`, `frameId`, `boundElements`, arrows with relative `points` and `startBinding`/`endBinding { elementId, focus, gap }`, text with `containerId`; plus `files` for images | a record store (`shape`, `binding`, `page` records) | mxGraph XML (compressed by default) |

Libraries checked against their registries (2026-10-18):

| Library | Version | Licence | Use |
| :--- | :--- | :--- | :--- |
| `perfect-freehand` (npm) / `perfect_freehand` (crate) | 1.2.3 / 0.1.1 | MIT | already used for freehand outlines, kept |
| `roughjs` (npm) | 4.6.6 | MIT | **not added**: we reimplement its idea (two jittered passes, bowing) in ~150 lines per language so the canvas preview and the Rust export draw the *same* seeded strokes; the algorithm is described in its MIT sources |
| `roughr` (crate) | 0.14.0 | MIT | not added, for the same reason; `roughr-merman` is in the tree through merman |
| `dugong` (crate, port of dagre) | 0.9.0 (2026-10-12) | MIT OR Apache-2.0 | **added** to manang-figure for `figure.layout`: already in the tree through merman, pure Rust, no I/O, compiles to wasm32 |
| `kurbo` | 0.13.1 | MIT OR Apache-2.0 | in the tree; not needed (our curves are cubic Béziers we emit as path data) |
| `resvg` / `tiny-skia` | 0.47.0 / 0.12.0 | Apache-2.0 OR MIT / BSD-3 | in the tree through merman; PNG export deferred (needs font data, see Deferred) |
| `tldraw` (npm) | 5.5.2 | "SEE LICENSE IN LICENSE.md" (tldraw licence) | rejected: source-available, needs a licence key in production |
| `@excalidraw/excalidraw` (npm) | 0.18.1 | MIT | rejected: React peer dependency, its own scene model and renderer (ADR 0001) |

## Decisions

### Scope

**v1 (this ADR):**

* Tools: select (V), hand (H), rectangle (R), diamond (D), ellipse (O),
  arrow (A), line (L), freedraw (P), text (T), frame (F), eraser (E), laser
  (K), formula (M), image (paste or drop), Mermaid (insert as shapes or as a
  block). Keys follow Excalidraw where they do not clash.
* Arrows and lines bind to shapes at either end and re-route (in Rust) when a
  bound shape moves, resizes or rotates; routing straight, elbow
  (orthogonal) or curved; arrow heads none/arrow/triangle/dot/bar at either
  end.
* Labels inside rectangles, diamonds, ellipses and frames, and on arrows;
  wrapped to the shape with deterministic metrics.
* Style per shape: stroke and fill colour (named palette tokens or `#rrggbb`),
  stroke width, stroke style (solid, dashed, dotted), opacity, roughness
  (0 clean, 1 sketchy, 2 very sketchy), font (sans or mono), font size,
  text alignment.
* Multi-select, marquee, group/ungroup (nested), z-order, align, distribute,
  lock, rotation and resize handles, snapping to other shapes' edges and
  centres with guides, copy/paste/duplicate, undo/redo, infinite canvas
  with zoom/pan, zoom to fit and to selection, frames as export regions.
* Export: SVG (always, deterministic) and PDF (through the existing
  `project.export-pdf` path), with or without background, theme `print`
  (default for files), `light` or `dark`; one frame or the whole scene.
* Mermaid: flowchart, state, class (native shapes + bound arrows) and
  sequence (actors, lifelines, messages) converted to native shapes; any
  family as a Mermaid block.
* Agent tools (below), documented in `docs/plugins.md`.

**Later:** real-time collaboration, image cropping, element libraries
(`.figurelib`), PNG export, Excalidraw import UI (the format is close enough
that a converter is a small step; see "Import"), hachure fills, rounded
corners, flipping, arrow waypoint editing, elbow arrows that avoid
obstacles, presentation mode over frames.

### Scene format v2

One JSON object per `<name>.figure.json`, pretty-printed with two-space
indentation and a final newline, keys in a fixed order, numbers rounded to
two decimals on write, defaults omitted, so a one-shape edit is a small
diff:

```json
{
  "version": 2,
  "width": 960,
  "height": 540,
  "nodes": [
    { "kind": "rect", "id": "n1", "x": 40, "y": 40, "width": 160, "height": 80,
      "label": "Start", "style": { "fill": "surface" } },
    { "kind": "diamond", "id": "n2", "x": 300, "y": 30, "width": 140, "height": 100,
      "label": "Valid?", "angle": 0, "groups": ["g1"], "locked": false },
    { "kind": "arrow", "id": "n3",
      "points": [{ "x": 200, "y": 80 }, { "x": 300, "y": 80 }],
      "start": { "node": "n1" },
      "end": { "node": "n2", "anchor": { "x": 0, "y": 0.5 } },
      "routing": "straight", "heads": { "end": "arrow" }, "label": "yes" }
  ]
}
```

* `width`/`height` are the page: the export region when there is no frame
  and everything fits on it (otherwise the content's bounds plus a margin).
* `nodes` are in paint order (first is bottom). Ids are `n<base36>`, from a
  counter, so replaying a session gives byte-identical files.
* Common optional fields: `angle` (degrees, clockwise, about the bounding
  box centre), `groups` (group ids, innermost first, Excalidraw's model:
  groups are not nodes, so z-order stays one flat list), `locked`, `style`,
  `label`.
* Kinds: `rect`, `diamond`, `ellipse`, `text`, `frame`, `image`, `mermaid`
  (box kinds: `x`, `y`, `width`, `height`); `arrow`, `line` (absolute
  `points`, `start`/`end` bindings, `routing`, `heads`); `freedraw`
  (`points` with pressure, `size`); `math` (`x`, `y`, `tex`, `size`).
  Box kinds share one shape: `label` is the text inside a shape, the text of
  a `text` node and the title of a `frame`; `image` has `src` (a path
  relative to the figure, or a `data:` URI for small pasted images);
  `mermaid` has `source`.
* `style`: `stroke`, `fill`, `strokeWidth`, `strokeStyle`, `opacity`
  (0-100), `roughness` (0-2), `fontFamily` (`sans`, `mono`), `fontSize`,
  `textAlign`. All optional; defaults are `ink`, `none`, 1.5, `solid`, 100,
  0, `sans`, 16, `center` (`left` for `text`).
* **Reading v1.** A file with `"version": 1` (or no version) is migrated on
  read: `stroke` becomes `freedraw` (its `color` the stroke colour), `rect`
  keeps its box and moves `stroke`/`fill` into `style`, `ellipse` turns its
  centre and radii into a box, `arrow`'s `from`/`to` become two unbound
  `points` with an arrow head at the end, `math` moves `color` into `style`.
  A v1 negative-size rectangle is normalised. Hex colours stay hex. The
  file is written as v2 on the next save; the migration is lossless for
  what v1 could express (property-tested).
* Unknown fields are rejected on read (`deny_unknown_fields`), so a typo in a
  hand-edited file is an error with a line, not silently dropped data.

### Colour model

Figure *content* may use colour; UI chrome may not. The palette is declared
once, in `crates/manang-figure/src/palette.json`, read by the Rust renderer
(`include_str!`) and by the frontend (JSON import), and checked by a theme
test (`src/lib/theme/figurePalette.test.ts`) that every figure source uses
tokens or this file, never stray hex codes. Each token has a `dark`, `light`
and `print` value:

* neutrals `ink`, `muted`, `line`, `surface`, `paper` (the workspace tokens);
* `accent` (acid lime on dark, its ADR 0002 dark mix on light/print) for the
  one thing that matters;
* the Okabe-Ito colour-blind-safe hues `orange`, `sky`, `green`, `yellow`,
  `blue`, `vermillion`, `purple`: distinguishable in print and for readers
  with colour-vision deficiency, which matters in scientific figures.

A hue used as a fill paints as a 20% tint; neutrals fill solid. A stroke or
fill may also be a literal `#rrggbb` (v1 files, pasted Excalidraw scenes).
The canvas draws with the app theme's values; files are written with
`print` (paper background off by default) because they are embedded in
documents.

### Rendering

`manang_figure::render_svg(scene, options, embeds)` is the one renderer for
files and exports. It is a pure function: no clock, no randomness, no I/O.

* Shapes are explicit geometry (`<rect>`, `<path>`), never markers or CSS,
  so Typst, usvg and LaTeX (through the PDF) draw them identically.
* **Hand-drawn style.** With `roughness > 0` each outline is drawn as two
  jittered passes (the rough.js method: endpoints offset, midpoint bowed,
  ellipses as perturbed curve points), from a xorshift32 PRNG seeded with a
  hash of the node id. The same code exists in TypeScript
  (`src/lib/figure/sketch.ts`) so the canvas shows exactly what is
  exported; parity is tested against shared fixtures.
* **Text.** Labels wrap at the shape's width with per-character advance
  tables approximating DM Sans and DM Mono (the app ships those fonts only as
  WOFF2, so there is no TTF for `ttf-parser`; see ADR 0011). Both languages
  share the tables, so canvas and export break lines at the same words.
* **Frames** export as regions: `figure.export { frame }` crops the SVG to a
  frame's box and draws only what it contains.
* **Mermaid blocks** are rendered by `diagram.render` (theme matching the
  export) and embedded as a nested `<svg>` at the block's box. The renderer
  receives them through `embeds`, so it stays pure and wasm-friendly; the
  plugin fills `embeds` through `Context::invoke`.
* **Canvas.** The frontend draws the same scene with the same geometry
  modules (`geometry.ts`, `sketch.ts`, `text.ts`), node by node in SVG,
  culled to the viewport. Pointer moves change a local preview only; one
  operation is sent on pointer-up.

### Operations and history

`figure.apply { id?, op }` takes one ID-addressed operation:

| `op` | Fields | Notes |
| :--- | :--- | :--- |
| `add` | `nodes`, `index?` | ids must be new |
| `update` | `id`, `set` | shallow merge of fields (and of `style`); `null` clears an optional field |
| `remove` | `ids` | arrows bound to a removed shape keep their position and lose the binding |
| `move` | `ids`, `dx`, `dy` | |
| `resize` | `id`, `x`, `y`, `width`, `height` | box kinds set the box; points kinds scale |
| `rotate` | `ids`, `angle` | absolute degrees |
| `reorder` | `ids`, `to` (`front`, `back`, `forward`, `backward`) | |
| `group` / `ungroup` | `ids`, `group?` | ungroup removes the outermost group |
| `bind` | `id`, `end` (`start`/`end`), `node` (or null), `anchor?` | |
| `align` | `ids`, `edge` (`left`, `center`, `right`, `top`, `middle`, `bottom`) | |
| `distribute` | `ids`, `axis` (`horizontal`, `vertical`) | equal gaps |
| `set-style` | `ids`, `style` | merge |
| `set-canvas` | `width`, `height` | |
| `batch` | `ops` | one undo step |
| `clear`, `undo`, `redo` | | |

The v1 ops (`add-node`, `update-node`, `remove-node`, `translate`,
`set-style { id, stroke, fill, size, color }`) are still accepted.

Every applied operation records a **delta**: the before and after value of
each node it touched (including the arrows it re-routed), and the order of
ids when the operation changed it. Undo applies the before side, redo the
after side, so undo of any operation restores the scene exactly
(property-tested on random operation sequences) and memory grows with what
changed, not with the scene (a 2,000-shape scene does not keep 100 copies).
The result reports `{ revision, applied, can_undo, can_redo, changed }`.
Edits from anything other than the editor (agent tools, other plugins) emit
`figure.changed { id, revision }` so an open tab resyncs.

### Arrow binding

A binding is `{ node, anchor? }`. Without `anchor` the end is *floating*:
it sits where the line from the other end's reference point to the target's
centre crosses the target's outline (rectangle, diamond, ellipse, rotated
or not), plus a 4-unit gap. With `anchor` (`x`, `y` in 0..1 of the box) the
end is *fixed* to that point of the outline, like tldraw's precise
bindings. Routing:

* `straight`: start and end only.
* `elbow`: an orthogonal path leaving and entering perpendicular to the
  sides the ends sit on (two or three segments, chosen from the relative
  positions); no obstacle avoidance in v1.
* `curved`: a cubic Bézier through the ends with control points along the
  exit and entry directions (or a Catmull-Rom spline through imported
  Mermaid waypoints).

Rust re-routes after every operation that changes a bound shape's geometry,
inside the same delta, so undo puts the arrows back too. The canvas runs the
same routing (`binding.ts`) during a drag so arrows follow live.

### Mermaid

* **As shapes** (`figure.from-mermaid`, "Insert Mermaid", and pasting Mermaid
  text then choosing "Insert as shapes"): the figure plugin asks the diagram
  plugin for `diagram.layout { source }`, a new command that returns
  merman's own laid-out geometry (`RenderRequest::layout_json`: node centres
  and sizes, edge waypoints, clusters) joined with the semantic model
  (labels, shapes, edge labels, dashes). Flowchart nodes map to rect,
  rounded rect (rect), diamond, ellipse (circle/stadium) shapes with
  labels; subgraphs become frames; edges become arrows *bound* to their
  shapes, with labels and dashed style. State diagrams map the same way
  (start and end states as small ellipses); class diagrams become boxes
  whose label lists members and methods, relations become bound arrows;
  sequence diagrams become actor boxes, dashed lifelines and message arrows.
  Every other family (and any layout failure) falls back to a Mermaid block.
  merman measures labels with its own metrics; shapes keep its sizes.
* **As a block**: a `mermaid` node keeps its source, renders through
  `diagram.render`, is edited as text in the properties panel and
  re-rendered on change.
* `figure.to-mermaid` writes a flowchart from boxes, diamonds and ellipses
  and the arrows between them (best effort; freedraw, images and unbound
  arrows are listed as skipped).

### Agent tools

The kernel already had a fan-out hook `register_agent_tool` with no payload
contract. The contract is now:

* A plugin subscribes to the hook in `manifest.toml` and answers it with
  `{ "tools": [AgentTool, ...] }`.
* `AgentTool` = `{ name, title, description, input_schema, output_schema,
  command }`. `name` matches `^[a-zA-Z0-9_-]{1,64}$` (accepted by MCP clients
  and model APIs), `command` is one of the plugin's own commands,
  `input_schema` and `output_schema` are JSON Schema (draft 2020-12) objects.
  The shape is MCP's `Tool` (`name`, `title`, `description`, `inputSchema`,
  `outputSchema`) plus the command to run.
* `Kernel::agent_tools()` collects them from every subscriber (one broken
  plugin does not hide the others; malformed tools are reported, not
  listed). `Kernel::call_agent_tool(name, input)` validates `input`
  against the schema (a small validator for the subset tools use: `type`,
  `properties`, `required`, `additionalProperties`, `enum`, `items`,
  `minimum`, `maximum`, `minItems`, `maxItems`, `oneOf`, `anyOf`) and runs
  the command. Validation errors are structured
  (`{ path, message }` per problem) so an agent can fix its call.
* The shell exposes the list as the `kernel_agent_tools` IPC command.

Figure tools, all ordinary `figure.*` commands, forgiving (shapes are
addressed by id *or* label; positions are optional and auto-placed), each
one undo step, each emitting `figure.changed`:

| Tool | Command | Does |
| :--- | :--- | :--- |
| `figure_describe` | `figure.describe` | shapes (id, kind, label, box), connections (from, to, label), groups, frames, and a one-paragraph text summary |
| `figure_add_shapes` | `figure.add-shapes` | add several shapes (kind, label, optional position/size/style); returns ids |
| `figure_connect` | `figure.connect` | arrows between shapes by id or label, with routing, label and style |
| `figure_update` | `figure.update` | change label, text, position, size or style of shapes by id or label |
| `figure_remove` | `figure.remove` | remove shapes by id or label |
| `figure_layout` | `figure.layout` | layered (dagre) layout of the shapes and arrows, `TB`/`LR`/`BT`/`RL` |
| `figure_from_mermaid` | `figure.from-mermaid` | Mermaid text to native shapes (or a block) |
| `figure_to_mermaid` | `figure.to-mermaid` | the figure as Mermaid flowchart text |
| `figure_style` | `figure.style` | apply a named preset (`clean`, `sketch`, `emphasis`, `muted`, `dashed`, `blueprint`) or explicit style |
| `figure_export` | `figure.export` | save the scene and write SVG (and PDF) next to it |

Every tool takes the figure as `path` (opened if needed) or `id`, else the
active figure. Errors name what was wrong and what would work, for example
`no shape matches "Strat"; did you mean "Start" (n1)?`.

### Performance targets

* 60 fps with 2,000 shapes on screen; < 8 ms per pointer move (preview only,
  no IPC).
* `figure.apply` of one operation on a 2,000-shape scene < 1 ms; rendering
  2,000 shapes to SVG < 20 ms in the clean style (criterion bench
  `figure_ops`).

Measured (2026-10-18, dev machine, `cargo bench -p manang-figure`): one move
on a 2,000-shape scene with 667 bound arrows 0.12 ms; 1,000 operations with
undo/redo history 88 ms (0.09 ms each); SVG export of 2,000 shapes 11.8 ms
clean, 25.5 ms hand-drawn (the hand-drawn style draws every edge twice; it is
an export-time cost, not a per-keystroke one).

## Alternatives rejected

* **Excalidraw as a component**: React-bound, its own state model; the
  scene would no longer be ours to render in Rust.
* **tldraw**: source-available, licence key, React.
* **Snapshots for undo**: simple, but 100 copies of a large scene break the
  memory budget.
* **A canvas/WebGL renderer**: SVG is what we export, it keeps text crisp and
  selectable, and with culling it holds the target. Revisit past 10,000
  shapes.
* **Linking merman into manang-figure**: it would double the figure crate's
  compile cost and tie the web build to it; the kernel composes plugins
  instead (`diagram.layout`).

## Import

Excalidraw's JSON maps almost one to one (`rectangle`, `diamond`, `ellipse`,
`arrow`, `line`, `freedraw`, `text`, `frame`, `image`; `x`, `y`, `width`,
`height`, `angle`, `strokeColor`, `backgroundColor`, `strokeWidth`,
`strokeStyle`, `roughness`, `opacity`, `groupIds`; bindings by
`elementId`; arrow points relative to `x`/`y`; bound text as labels). An
importer is deferred but the v2 fields were chosen so it is a mapping, not
a redesign.

## Consequences

* `dugong` becomes a direct dependency of manang-figure (already in the
  lockfile).
* The browser mock (`mockFigure.ts`) mirrors the operations through
  `src/lib/figure/ops.ts`, the same module the editor uses for optimistic
  edits; a shared fixture (`crates/manang-figure/tests/fixtures/parity.json`)
  generated by Rust keeps the two in step.
* Old figures open unchanged and are upgraded on their next save.

## Amendment 2026-10-18: formulas, colours, polish

* **Formulas are real.** A `math` node keeps `source`, `language`
  (`typst` default, omitted in files; `latex`), `size`, a measured `width` and
  `height` (0/absent until first rendered) and a top-left `x`,`y`. The
  rendering is never stored: it is vector paths, cached in memory by
  `(language, source, size)` in Rust and `(language, source, size, colour)`
  on the canvas. Scene version stays 2; nodes that still have `tex` are read
  as LaTeX `source` with `y` moved from baseline to top (same box). Formulas
  bind to arrows like shapes and scale their `size` with resize.
* **Typst math** renders through the new `typst.math-to-svg { source, size?,
  color? }` (in-memory one-line document, auto page, embedded fonts only, no
  clock, outlines, text paint replaced by an XML pass). **LaTeX math** renders
  in process with RaTeX (`ratex-*` 0.1.15, MIT, pure Rust, KaTeX fonts
  embedded; already in the tree through merman). Chosen over going through
  `manang-diagram` because it adds no new crate, needs no kernel hop and
  `manang-figure` still checks for `wasm32-unknown-unknown`; sources with
  CJK/emoji are declined so RaTeX never scans system fonts. Errors are
  `[math_error] {message,line,column,length}` inside the formula.
  `figure.render-math {language, source, size, color?}` serves the canvas;
  export nests the paths in the node's colour. Browser mock: approximate,
  labelled rendering (`mathApprox.ts`).
* **Colours.** The stored colour stays a string: a token or lowercase
  `#rrggbb` (normalised and validated on read). Style gains `labelColor`
  (default: stroke) and `fillOpacity`; the scene gains `background`
  (`set-canvas { background }`, `none` clears); an exported frame draws its
  own fill. Recent custom colours live in localStorage, not the scene.
  Print values of orange, sky and yellow were darkened to 3:1 on white.
* **Polish.** Diamond labels use a 60% text box, break at spaces and shrink
  the font (to 70%) before breaking a word; properties dock right and
  collapse; multi-selections scale from their handles; Mermaid inserts to
  the right of the content and is revealed.
