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
