# ADR 0006: Show LaTeX PDFs with pdf.js in a lazily loaded viewer

Status: accepted, 2026-10-17

## Context

`latex.export-pdf` returns the PDF as raw bytes. The preview needs to show it
with page navigation, zoom, fit width and a stable scroll position across
recompiles, on long documents, without network access and without linking a C
library into the app binary (AGENTS.md 7.4: Tectonic's static HarfBuzz crashed
GTK/WebKit). Candidates, checked on 2026-10-17:

- **pdf.js** (`pdfjs-dist` 6.4.299, published 2026-10-03, Apache-2.0). Pure
  JavaScript; no native code reaches the app. Parses and rasterises in a Web
  Worker, so the UI thread only paints canvases. Package is 35 MB unpacked, but
  the build ships `pdf.min.mjs` (459 kB, 129 kB gzip) and `pdf.worker.min.mjs`
  (1.26 MB). Nothing is fetched at runtime unless a PDF needs optional assets
  (JPEG 2000 / JBIG2 WASM, CJK cmaps, non-embedded standard fonts); LaTeX output
  embeds its fonts and uses JPEG/PNG images, so none of that is needed.
- **hayro** 0.8.0 (crates.io, Apache-2.0 OR MIT, 2026-10-04, MSRV 1.92). A pure
  Rust rasteriser (`vello_cpu`, `kurbo`, `image`); no C dependencies. It would
  keep the work in Rust, but it produces bitmaps: every page at every zoom and
  display scale crosses IPC as raw pixels (a 2x A4 page is about 8 MB), or the
  text layer and selection are lost if we vectorise instead. It also needs a new
  Rust plugin crate and a render cache in the app process, whose idle memory
  budget is 75 MB, and rasterising competes with Typst for the Rayon pool.
- A WebView-native PDF viewer (`<embed>`/`<iframe>`) was rejected: WebKitGTK
  has no built-in PDF plugin, and CSP is `object-src 'none'`.

## Decision

Use **pdf.js**, wrapped in `PdfViewer.svelte`, behind a dynamic `import()` so
nothing is loaded until a LaTeX preview is shown (the main bundle is unchanged).

- **Worker set-up with Vite:** the worker file is imported as a URL
  (`pdfjs-dist/build/pdf.worker.min.mjs?url`) and assigned to
  `GlobalWorkerOptions.workerSrc`; Vite emits it as a same-origin asset. No CDN,
  no network (`useWorkerFetch: false`).
- **Strict CSP unchanged:** `default-src 'self'` already covers `worker-src`;
  pdf.js 6 has no `eval` path, so `script-src` stays without `unsafe-eval`;
  pages draw on `<canvas>`, so `img-src` needs nothing new.
- **Memory stays flat:** only pages within one viewport of the visible area own
  a canvas (an `IntersectionObserver` with a 100% margin); a page that leaves
  gives its canvas back and `cleanup()`s its pdf.js resources. Canvas scale is
  capped at 2x. Placeholder slots are plain empty elements of the right size, so
  a 500-page document costs 500 small `<div>`s. The raw PDF bytes are kept once
  (a copy is handed to the worker, which owns it).
- **Scroll position across recompiles:** the scroll container and slots are not
  remounted; a new document swaps in and each visible page redraws over the old
  canvas, so the reader stays where they were. Verified in the browser harness.
- **Bytes stay binary:** `latex.export-pdf` reaches the viewer as a `Uint8Array`
  (`tauri::ipc::Response`), never base64.

## Consequences

- LaTeX previews cost about 1.7 MB of lazily loaded JavaScript the first time;
  Typst and Markdown users never pay it. Not measured: heap use of a very long
  document in the real WebView (the harness checks at most 4 live canvases).
- pdf.js 6 targets current browser engines; WebKitGTK as shipped by current
  distributions is fine, and `pdfjs-dist/legacy` is the fallback if an older
  WebKit must be supported.
- Text selection and links are not drawn yet (no text layer). SyncTeX click
  jumps (`latex.synctex`) are a follow-up and need page/point mapping from this
  viewer.
- If a Rust renderer is wanted later (for example a thumbnail strip), hayro is
  the candidate; it can sit behind the same `latex.export-pdf` bytes.

## Amendment, 2026-10-18: one viewer for Typst and LaTeX, Overleaf parity

### Decisions

- **Typst previews as a PDF.** The one-page SVG preview (`typst.render` into
  an `<img>`) is gone from the preview pane; Typst and LaTeX both go through
  this viewer, so selection, find, links, the outline and zoom exist once.
  `typst.export-pdf` already exported from the cached `PagedDocument` and
  caches the bytes per compile, so no new Rust path was needed. `typst.render`
  stays for project thumbnails and slides. Typst's jump commands speak points
  from the page's top-left corner, which is what the viewer measures; a Rust
  test checks that every exported page box is `[0 0 w h]` with the compiled
  size and a vitest reads the exported PDF with pdf.js and finds the headings
  where `typst.jump-from-cursor` puts them (`testdata/typst-preview/`).
- **Text layer:** pdf.js `TextLayer` per drawn page, styled after pdf.js'
  viewer CSS (transparent runs, `--total-scale-factor`, so it follows zoom
  without being rebuilt). Selection handling is pdf.js' `endOfContent`
  technique as adapted in Theorem (`ensureGlobalTextLayerSelectionListeners`,
  MIT, same owner), with its edge auto-scroll and its WebKit text-width
  calibration (WebKitGTK measures fonts differently from pdf.js' canvas, so
  runs are re-scaled per font from measured widths). Copy writes tidied
  plain text. A page whose text is part of the selection keeps its layer when
  it scrolls away.
- **Links:** our own link layer from `getAnnotations()` (as Theorem's
  `PDFLinkLayer`), not pdf.js' `AnnotationLayer`: no link service, forms or
  scripting to carry. Internal destinations scroll inside the viewer; web
  links (http/https only) go to `openExternal`; the webview never navigates.
- **Zoom:** 100% is the printed size at 96 dpi (1 pt = 4/3 CSS px). Presets
  fit width, fit page, 50 to 400%; Ctrl/Cmd+=, -, 0; Ctrl+wheel and pinch
  are previewed with a CSS transform and committed when the gesture pauses
  (Theorem's approach, `wheelZoomFactor` too), anchored so the point under the
  pointer stays put. The wheel listener is not passive, so the webview never
  zooms itself. Pages relayout at once (canvases stretch) and are redrawn at
  zoom x devicePixelRatio 140 ms after the zoom settles.
- **Canvas resolution:** the 2x cap is replaced by a pixel budget of 2^24
  pixels (64 MB) per page canvas; only pages within one viewport of the
  visible area own a canvas, a text layer and links, as before.
- **One pdf.js worker** is shared by every document. A worker per
  `getDocument` cost about 200 ms of worker start-up on every recompile.
- **Double-click** goes to the source by default (setting
  `preview.double-click-source`, like Overleaf); Alt+double-click selects a
  word. **Dark pages** (setting `preview.dark-pages`, header toggle) inverts
  the canvas only (`invert(1) hue-rotate(180deg)`); export is unaffected.
- The pure parts are in `src/lib/workbench/pdf/` (zoom, layout and anchoring,
  navigation and the page box, find, links and outline, calibration) with
  vitest suites; state in `state/pdfView.svelte.ts`.
- No fonts or cmaps are shipped: Typst and pdfLaTeX output embed their fonts.
  A PDF with non-embedded CJK text may still need cmaps later.

### Measured (2026-10-18, this machine: 12 threads, optimised dev profile)

Edit to updated visible pages, for Typst documents of 11 and 52 A4 pages
(headings, outline, links, `lorem`), split pane, two pages visible:

| Step | 11 pages | 52 pages |
| :--- | ---: | ---: |
| Incremental compile (`cargo bench --profile dev --bench pdf`) | 6-12 ms | 1-8 ms |
| `export_pdf` after the edit | 7.5 ms | 33 ms |
| pdf.js open with the shared worker (Chromium) | 2-6 ms | 4-5 ms |
| Draw the two visible pages: canvas, text layer, links | 95-108 ms | 198-210 ms |
| Main-thread tasks over 50 ms during the draw | 0 | 0 |

So an edit shows in about 120 ms (11 pages) and 250 ms (52 pages) after the
debounce, most of it drawing (fonts are loaded again for each new document);
the drawing is sliced by pdf.js, never blocks the main thread for 50 ms, and
the old pages stay on screen until the new ones are drawn, so typing does not
stutter. The SVG fallback was therefore not kept.

Memory at 400% on a 3-page document in the browser harness: 2 canvases,
28.5 M pixels in all, 35 MB JS heap; at fit width 3-4 canvases. The WebKitGTK
numbers in the real app are not measured here (headless Chromium only).
