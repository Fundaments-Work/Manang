# Visual editor and document-format plugins

Date: 2026-10-18. Branch `agent/research-visual-editor`. Extends
`docs/plans/2026-10-18-platform-v2.md` (sections 3.2, 3.3, 3.4 and 6 stay as
decided). Facts marked **[V]** were fetched from a primary source this session
(URL given); **[M]** are from memory and need a check before they go in an ADR.

## Verdict

1. **One visual-editor plugin, per-format adapters, source stays the only
   truth.** For every text format (Typst, LaTeX, Markdown, later AsciiDoc, Org,
   reStructuredText) the visual layer is CodeMirror decorations over the real
   source, driven by a syntax tree, and every toolbar action produces ordinary
   *text edits*. Nothing is converted to a document model and serialised back.
   This is how Overleaf and Obsidian do it, and it makes git diffs, undo and
   the Loro CRDT correct by construction.
2. **`.docx` is the opposite case** and is a separate document-model plugin
   (ProseMirror-class model in the webview, OOXML in Rust) built on the same
   contribution points. Do not try to unify the two editing engines; unify
   the contribution points, the toolbar vocabulary and the outline/comment
   services.
3. **Our winning feature is the adapter contract**: a third party writes ~300
   lines of Rust (parse to spans, a few edit commands) and gets live preview,
   toolbar, outline, math widgets, figures, citations, comments and co-editing
   for their format. That needs one amendment to the plugin runtime
   (stateful sessions, section 4.4) and a real developer experience (section 5).
4. **Beta slice (3-4 weeks):** Markdown then Typst, native first-party
   adapters, viewport span table over binary IPC, toolbar, math/figure/link
   widgets. LaTeX in the following milestone. Tables and docx deferred.

## 1. How the existing editors work

### Overleaf Visual Editor (closest reference)

* Overleaf moved its source editor to CodeMirror 6 explicitly as "a crucial
  step" towards unifying it with the Rich Text editor, so both improve in
  parallel **[V]** (<https://www.overleaf.com/blog/towards-the-future-a-new-source-editor>).
  So visual mode is CM6 plus a Lezer grammar for LaTeX (`lezer-latex/latex.grammar`
  plus `tokens.mjs`, compiled by `@lezer/generator`) **[V]**
  (<https://github.com/overleaf/overleaf/tree/main/services/web/frontend/js/features/source-editor/lezer-latex>).
* The visual layer is a set of extensions in `extensions/visual/`:
  `atomic-decorations.ts`, `mark-decorations.ts`, `list-item-marker.ts`,
  `visual-keymap.ts`, `table-generator.ts`, `paste-html.ts`, `visual-widgets/`
  **[V]** (directory listing). `createDecorations(state, tree)` walks the Lezer
  tree with `tree.iterate` and emits `Decoration.replace`/`Decoration.widget`
  for sectioning commands, `\cite`, `\ref`, `\label`, `\includegraphics`,
  figure/table/list environments, tabulars (`TabularWidget`, or
  `TableRenderingErrorWidget` when the table cannot be parsed) and math
  (`MathWidget`, children not decorated) **[V]**
  (<https://github.com/overleaf/overleaf/blob/main/services/web/frontend/js/features/source-editor/extensions/visual/atomic-decorations.ts>).
* **Cursor reveals source.** A widget is skipped when the selection touches
  its range (`shouldDecorate`), so the raw LaTeX appears exactly where you are
  typing **[V]** (same file).
* **Unsupported LaTeX is simply not decorated**: unknown commands and
  environments fall through to the source. Overleaf's help text says content it
  cannot render, such as some tables, complex package code, journal template
  code and custom macros, "often appears as raw LaTeX source", and a broken
  table shows "Sorry, your table can't be displayed at the moment"; users are
  told to switch to the Code editor for complex formatting **[V]**
  (<https://docs.overleaf.com/index.md?ask=...>, the docs' own answer endpoint).
  Preamble custom commands are collected and passed to the math widget so
  macros render in formulas **[V]**.
* Limits visible in the code: mode toggling reconfigures compartments (visual
  extensions are dropped, not hidden); the whole document is force-parsed
  before the visual view becomes editable, with a 5 s fallback, which blocks
  on large files **[V]**
  (`visual.ts`, `showContentWhenParsed`). A viewport-limited design avoids that.
* Licence: the repository is AGPL-3.0 **[V]**
  (<https://github.com/overleaf/overleaf>). We may study the design; we must
  not copy code (Manang is MIT).

### Obsidian Live Preview, Typora

Obsidian's default editing mode "shows formatted text inline while hiding most
Markdown syntax", and the syntax reappears when the cursor enters it; Source
mode shows the raw Markdown **[V]** (<https://obsidian.md/help/edit-and-read>).
Same mechanism as Overleaf (CM6 decorations, reveal at cursor). Typora is
closed source and works the same way from the user's view **[M]**.

### Document-model editors (the lossy family)

* **Quarto/RStudio visual editor**: Pandoc-markdown parsed into a ProseMirror
  model and written back through Pandoc **[M]**; the consequences are
  documented. The writer "may rewrite your Markdown into Pandoc conventions":
  asterisks for emphasis, escaped syntax characters, paragraphs on one line
  unless `wrap` is set, reference links become ordinary links, auto-numbered and
  example lists become plain numbered lists, inline footnotes become numbered
  footnotes; some title blocks make the document fail to load. `canonical: true`
  exists precisely to make visual and source saves produce identical text
  **[V]** (<https://quarto.org/docs/visual-editor/markdown.html>). That is the
  diff noise a source-preserving design avoids.
* **ProseMirror** documents are immutable trees validated by a schema; edits
  are steps between documents **[V]** (<https://prosemirror.net/docs/guide/>).
  Tiptap, Lexical and Notion-style blocks share this shape. Right for docx,
  wrong for LaTeX/Typst, where the schema cannot express what users write
  (macros, `#set`, scripting).
* **Typst web app**: source editor with live preview and click-to-jump; no
  WYSIWYG mode of its own **[V]** (<https://typst.app/docs/web-app/>, secondary
  summaries). Third-party WYSIWYG Typst tools exist; none is a reference.
* **LyX and TeXmacs** are document-model editors with their own file formats
  (LyX exports LaTeX; TeXmacs has its own tree format) **[M]**: round-trip of
  hand-written LaTeX is their known weakness.
* **Google Docs / Word** own a model and a binary-ish container; edits are
  operations on that model. This is the docx plugin's world.

### Recommendation per format family

| Family | Approach | Evidence |
| :--- | :--- | :--- |
| Markdown, Typst, LaTeX, AsciiDoc, Org, rST, BibTeX | Source-preserving decorations on a lossless tree; commands emit text edits | Overleaf, Obsidian; Quarto's rewrite list above |
| Quarto `.qmd` | Markdown adapter plus a fenced-div/chunk extension; never Pandoc AST round trip | Quarto's own `canonical` workaround |
| docx (OOXML) | Document model in webview, OOXML parts in Rust, preserve unknown parts | zip of XML, styles; no source text exists |
| HTML | Treat as source-decoration family, but only as read-mostly preview | html5ever has spans; low priority |

## 2. Parsers in Rust

| Format | Candidate | Lossless / spans | Notes |
| :--- | :--- | :--- | :--- |
| Typst | `typst-syntax` 0.15.1, already in tree (`manang-writing`) | **Yes.** `SyntaxNode` keeps every byte including trivia (`full_text()`, `children()`, `LinkedNode::range()`); error nodes keep the text; `Source::edit(range, text)` reparses incrementally and returns the reparsed range **[V]** (docs.rs, and read in `~/.cargo/registry/.../typst-syntax-0.15.1/src`). `Source::edit` panics on an out-of-range replace, so the adapter must clamp. | Compiles to wasm; the Typst web app does. Gives heading, strong, emph, list, term, raw, math, label, ref, function-call kinds. Best parser of the set. |
| Markdown | `pulldown-cmark` 0.13.4 (MIT, in tree) | `into_offset_iter()` yields `(Event, Range<usize>)` byte ranges **[V]**; it is an event stream, not a tree, and has no incremental reparse | Our `manang-markdown` already uses it. Fine: reparse a block range; document-level cost is milliseconds. Footnotes, tables, math, task lists via options. |
| Markdown (alt.) | `comrak` 0.56 (BSD-2) | `Sourcepos` is line/column only; the docs do not state byte offsets **[V]** | Real AST and CommonMark formatter, but positions are weaker. Skip. |
| Markdown (alt.) | `markdown` (markdown-rs) 1.0.0 (MIT) | mdast nodes carry an optional `position` **[V]** (offsets per unist **[M]**) | Good for MDX/frontmatter; slower and less used. Keep as a later option. |
| LaTeX | `tree-sitter-latex` 0.1.0 (MIT), grammar from texlab's parser, "best effort", states LaTeX is Turing complete and not everything can be handled **[V]** (<https://github.com/latex-lsp/tree-sitter-latex>) | Tree-sitter trees are lossless by byte range and recover from errors; incremental edits are native | C runtime (`cc` build dep), so native tier only, not `wasm32-unknown-unknown`. Crate is a low-traffic republish (1.5k downloads) **[V]** (crates.io API); vendor the grammar rather than depend on the crate. |
| LaTeX (alt.) | `texlab` parser | rowan-based CST **[M]** | **GPL-3.0** **[V]** (<https://github.com/latex-lsp/texlab>): cannot link into an MIT app. Reference only. |
| LaTeX (own) | A small hand parser in Rust producing a span tree | Full control, wasm-clean | The realistic long-term answer for the *visual subset*: sections, environments, `\textbf`-class commands, `\cite`, `\ref`, `\includegraphics`, math delimiters, comments. 800-1500 lines. Unknown macros become opaque `command` spans. |
| HTML | `html5ever`/`scraper` | spans are not a first-class output **[M]** | Defer. |
| docx | `ooxmlsdk` 0.13.2 (MIT OR Apache-2.0) | Typed parts; unloaded parts keep their original compressed bytes on save, schema extension content is preserved, 5,477 corpus files with zero failures reported; unknown-element DOM editing is not exposed; pre-1.0 **[V]** (<https://docs.rs/crate/ooxmlsdk/latest>) | Best round-trip story. |
| docx (alt.) | `docx-rs` 0.4.22 (MIT) | Reads into its own `Docx` model, JSON-serialisable; "OOXML is a large specification, so support is not exhaustive"; preservation of unknown parts is not documented **[V]** (<https://github.com/bokuweb/docx-rs>) | Good writer, risky for read-modify-write. |
| docx (alt.) | `docx-rust` 0.1.11 (MIT) | not verified | Skip. |

**Decision for LaTeX.** The user-facing risk is not parsing, it is *macros*.
Both tree-sitter and a hand parser see `\mycmd{x}` as a command; neither
knows it is a heading. Overleaf's answer is the same: it falls through to
source, and it extracts `\newcommand` definitions only to feed the math
renderer **[V]**. Copy that behaviour: the adapter reports `opaque` for
unknown commands, and a project-level "macro hints" table (name -> role)
in settings lets a user promote `\mysection` to a heading later.

**Why docx cannot use source decorations.** A `.docx` is a zip of XML parts
(`document.xml`, `styles.xml`, `numbering.xml`, relationships, media, comments,
settings). There is no linear text a user would diff or a cursor position that
maps to bytes; formatting is run properties and style inheritance; pagination
is layout, not content. The honest design is a model: parse parts to a typed
tree, edit through operations, serialise changed parts, and copy untouched
parts byte-for-byte. Minimum viable scope: paragraphs and runs (bold, italic,
underline, links), headings by style, bullet and numbered lists, simple
tables, inline images, comments (`comments.xml`), and track changes
(`w:ins`/`w:del` shown, accept/reject; never silently flatten). Everything
else is preserved, not editable.

## 3. Architecture: where the logic lives

### Web view versus Rust

AGENTS.md puts parsing, trees and commands in Rust. CodeMirror decorations
are DOM objects and must be built synchronously inside a `ViewPlugin`, so the
split is:

| Rust (adapter, native or WASM) | Web view (thin) |
| :--- | :--- |
| parse, incremental tree, kinds and ranges | `Decoration.replace/widget/mark` built from a span table |
| `blocks(viewport)`: the semantic span table | `StateField` holding the table, mapped through each `ChangeSet` |
| edit commands returning text edits | toolbar, keymap, cursor-reveal rule, widget DOM |
| outline, symbols, diagnostics | scroll/selection handling |
| math/figure rendering (Typst, RaTeX, figure SVG) | `<img>`/inline SVG from `Uint8Array` blobs |

**Keystroke path (budget 8 ms).** The editor already sends every user
transaction to the kernel as ops (`src/lib/editor/kernelSync.ts`), and the
rope lives in Rust. The visual layer adds no per-keystroke blocking call:

1. The transaction applies in CM; the existing decoration set is mapped through
   the `ChangeSet` (`Decoration.map`), so widgets stay attached and the
   frame renders immediately.
2. After the kernel applies the ops, the adapter session reparses the edited
   region (`Source::edit` for Typst) and the shell replies, or pushes an event,
   with a **delta span table** for the viewport plus margin, tagged with the
   document revision. Binary, `tauri::ipc::Response`, a flat array
   `(kind: u16, start: u32, end: u32, attr_ix: u32)`: 5k spans is about 60 KB.
3. CM drops the response if its revision is older than the local state,
   otherwise swaps the table in. At worst a widget shows stale for one frame;
   it is never wrongly *attached* because of step 1.

Estimated cost: incremental Typst reparse is microseconds to low milliseconds
(typst-syntax is designed for this); pulldown-cmark over a 50 KB viewport slice
about 1 ms. The measured loop in `docs/research/2026-10-18-live-spike.md` puts
the existing bridge at 10 us, so the budget is the span-table build and one IPC
hop, both off the typing frame. **This must be benchmarked in WP2 (criterion,
plus a Vitest timing test on a 1 MB document); it is an estimate.**

Alternative considered and rejected for Beta: run Lezer parsers in JS
(`@lezer/markdown`, `codemirror-lang-typst`, already used for highlighting)
and decorate from those. It is synchronous and zero IPC, but duplicates logic in
JS, cannot serve third-party adapters, and cannot reach typst semantics
(function calls, math mode). Keep it as the *highlighting* layer; the visual
layer reads the adapter's table. If measurements show IPC staleness is visible,
the escape hatch is compiling the same adapter crate to wasm for the web view
(also needed for the planned web version, AGENTS.md invariant 5).

### Cursor reveal, atomic ranges, selection

Copy the *behaviour* (not code) of Overleaf/Obsidian: a block is rendered
unless the selection intersects it (or its line, for list markers and
headings); widgets that replace text are registered as `atomicRanges`;
`Decoration.mark` for bold/italic/code wrap inline widgets. Everything inside
math, raw and code spans is opaque.

### Rendering widgets

* **Math**: `typst.math-to-svg { source, size?, color? }` already exists
  (`crates/manang-typst/src/plugin.rs`); LaTeX math through RaTeX (MIT, pure
  Rust, in `manang-figure/src/math.rs`). The adapter reports a `math` span with
  `lang = typst|latex`; the web view requests SVG bytes from a content-keyed
  cache and renders inline SVG. Markdown `$..$` follows the document's math
  language setting.
* **Figures**: `image`/`includegraphics`/`#image` spans with a `.figure.json`
  neighbour render the saved `.svg`; a click opens the figure studio tab.
* **Citations**: `cite` spans resolve through `manang-citations` to a short
  label ("Smith 2020") and a hover card.
* **Tables**: read-only rendered widget first (like Overleaf's `TabularWidget`
  and its error fallback), cell editing later as text edits to the cell range.
* **Comments**: stay outside the text (anchored to Loro cursors, see below).

## 4. Plugin architecture

### 4.1 Contribution points (additions to platform-v2 section 3.2)

Existing, unchanged: `document-types`, `importers`, `exporters`, `views`,
`commands`, `settings`, `keybindings`, `themes`. Proposed additions and
refinements:

```toml
[plugin]
name = "typst-format"
scope = "document"
tier = "native"                   # or wasm

[[contributes.document-types]]
id = "typst"
extensions = ["typ"]
language = "typst"                # names the adapter below
services = ["parse", "outline", "symbols", "diagnostics", "blocks", "edit-commands"]

[[contributes.format-adapters]]   # NEW: a source-text format adapter
id = "typst"
document-type = "typst"
kinds = ["heading", "strong", "emph", "list-item", "link", "math", "image",
         "cite", "ref", "raw", "table", "comment", "opaque"]
commands = ["bold", "italic", "heading", "list", "link", "insert-math",
            "insert-figure", "insert-citation", "insert-table"]

[[contributes.editors]]           # NEW
id = "visual"
kind = "visual"                   # source | visual | custom
document-types = ["*"]            # any type with a format adapter
title = "Visual"
entry = "builtin:visual-source"   # or "ui/editor.html" for custom
priority = "optional"             # never default-replaces source mode

[[contributes.toolbars]]          # NEW: declarative toolbar groups
editor = "visual"
items = [ { command = "visual.bold", icon = "bold", when = "adapter.has(bold)" } ]
```

The visual-editor plugin (`visual`, scope `document`) contributes the
`visual` editor and the generic commands `visual.bold`, `visual.heading`, ...
Each resolves through the *active document's adapter*. Format plugins
contribute only `document-types`, `format-adapters` and their importers.
`custom` editors (docx, figure) supply their own webview entry as in
platform-v2 section 3.4. `[[contributes.editors]]` with `kind = "source"` is
how the first-party CodeMirror editor itself registers, dogfooding the point.

Importers/exporters stay as decided: `docx <-> typst`, `md <-> typst`
converters register as importer/exporter pairs and run in Rust off the UI
thread (Pandoc-like converters are heavy; they belong in a sidecar or the
first-party tier, not a 250 ms WASM call).

### 4.2 The adapter interface (WIT sketch)

New package `manang:format@0.1.0`, separate from `manang:plugin` so existing
worlds keep working. Offsets are **UTF-8 byte offsets** into the document,
the same convention as the rope and the Loro `TextCrdt` trait.

```wit
package manang:format@0.1.0;

interface types {
    record range { start: u32, end: u32 }
    record span  { kind: u16, range: range, attr: u32 }   // attr indexes `attrs`
    record text-edit { range: range, replacement: string }
    record edit-plan { edits: list<text-edit>, selection: option<range> }
    variant failure-kind { invalid-argument, unsupported, internal }
    record failure { kind: failure-kind, message: string }
}

interface adapter {
    use types.{range, span, text-edit, edit-plan, failure};

    /// Long-lived per open document; created by the host, never by the UI.
    resource session {
        constructor(text: string);
        /// Apply a text change (ops from the rope) and reparse incrementally.
        edit: func(edits: list<text-edit>) -> result<range, failure>;
        /// Spans intersecting `viewport`, flat, plus a string table for attrs.
        blocks: func(viewport: range) -> result<span-table, failure>;
        outline: func() -> list<outline-entry>;
        diagnostics: func() -> list<diagnostic>;
        /// Which commands are enabled/active at this selection (toolbar state).
        command-state: func(selection: range) -> list<command-state>;
        /// Returns TEXT EDITS only. The host applies them to the rope.
        run-command: func(command: string, selection: range, args: string)
            -> result<edit-plan, failure>;
    }
    kinds: func() -> list<kind-info>;       // name, block|inline|atomic, reveal rule
    render-hint: func(kind: u16, attrs: string) -> render-hint;
}
```

`render-hint` says how a kind renders: `{ widget: heading | list-marker |
math{lang} | image{src} | cite{keys} | table | opaque, hide-markers:
list<range>, style: strong | emph | code | link }`. The web view has a fixed
vocabulary of widget kinds; an adapter cannot ship DOM, which keeps the
sandbox and the thin-UI rule intact. A `custom` kind opts into the plugin UI
webview (platform-v2 3.4) instead.

**The invariant.** `run-command` returns `edit-plan` only. The host applies
it as a rope transaction, so undo, the Loro CRDT, git and remote cursors see
ordinary text. A command can never write the buffer on its own, which also
means no new `buffer` permission is needed beyond reading. Example: `bold` on
a Markdown selection returns `[ {sel.start, sel.start, "**"}, {sel.end,
sel.end, "**"} ]`; on an existing bold it returns the two deletions.
Heading level 2 in Typst replaces the leading `=` run.

### 4.3 One plugin, three formats

The visual editor never mentions a format. For Typst the adapter wraps
`typst-syntax` (`SyntaxKind::Heading`, `Strong`, `Emph`, `ListItem`, `Equation`,
`FuncCall` named `image`/`figure`, `Ref`, `Label`); for Markdown it wraps
`pulldown-cmark` offset events; for LaTeX the hand parser or tree-sitter.
They all produce the same 14-ish kinds. A third-party AsciiDoc plugin
supplies a manifest with `document-types` + `format-adapters` and a component
exporting `adapter`; the language server-ish services (outline, symbols,
diagnostics) plug into the same panels as today. A format with no adapter
still opens in source mode, and the visual toggle is hidden.

Round-trip proof obligation (the property test every adapter must pass, using
`proptest` per AGENTS.md): for any document and any command, applying
`run-command`'s edits yields text whose adapter parse has no new errors and
whose untouched ranges are byte-identical.

### 4.4 Required runtime change

`docs/plugins.md` and ADR 0003 fix the **stateless-per-call** model (fresh
instance per call, 250 ms wall clock, nothing survives). Incremental parsing
is stateful. Options, in order of preference:

1. **Host-held session.** The host keeps one adapter *instance* alive per open
   document and routes `session` resource calls to it, with per-call time
   limits and a hard memory cap; it is rebuilt from the rope on any trap or
   hot reload. Cost: ADR 0003 gets an addendum ("stateful *document sessions*
   for adapters; commands and hooks stay stateless").
2. **Stateless full reparse of a viewport slice.** `blocks(text-slice)`. No
   runtime change, simple, adequate for Markdown-sized parsers (~1 ms), poor
   for Typst-scale trees. Use as the compatibility fallback for adapters that
   do not export `session`.
3. First-party native adapters bypass both (a Rust trait in `manang-kernel`),
   as `manang-typst` already does.

Recommend 1 with 2 as fallback and 3 for Beta; the ADR is part of WP1.

## 5. Plugin scaffolding as the product

### What exists today (verified in the repo)

* `crates/manang-plugins`: manager with `scan`, `watch` (250 ms debounce,
  atomic swap, old version keeps running on failure), `install`/`approve`/
  `enable`/`disable`/`uninstall` (to trash), `.manangplugin` zip archives with
  strict validation, consent UI and state, project-level disable, `PolicyGate`
  seam, `plugins.*` commands.
* `crates/manang-wasm-host`: Wasmtime component host, WIT `manang:plugin@0.2.0`
  (worlds `plugin`, `plugin-buffer`, `plugin-fs`, `plugin-http`,
  `plugin-full`), limits, MAC-protected `.cwasm` cache (0.2 ms warm load).
* `sdk/rust-plugin-template` (51-line `lib.rs`), `sdk/componentize`,
  `scripts/build-plugin.sh`, `plugins/word-count` (96 lines), agent tools via
  `register_agent_tool` with a JSON-schema validator, and
  `crates/manang-plugins/tests/sdk.rs` as the test pattern.

### Gaps against best in class

Comparison: VS Code `yo code` scaffolds a TypeScript extension and F5 opens an
Extension Development Host; edit then "Developer: Reload Window" **[V]**
(<https://code.visualstudio.com/api/get-started/your-first-extension>).
Obsidian: clone the sample plugin into the vault's plugins folder, `npm run
dev` rebuilds on save, the community Hot-Reload plugin reloads **[V]**
(<https://docs.obsidian.md/Plugins/Getting+started/Build+a+plugin>). Zed:
extensions are git repos with `extension.toml`, Rust compiled to wasm
(`wasm32-wasip2`) behind `zed_extension_api` and a `register_extension!` macro,
"Install Dev Extension" overrides a published copy **[V]**
(<https://zed.dev/docs/extensions/developing-extensions>). `cargo component`
is Apache-2.0 but "considered experimental", with plain `cargo` plus
`wasm32-wasip2` as the alternative **[V]**
(<https://github.com/bytecodealliance/cargo-component>).

| Gap | Proposal | Size |
| :--- | :--- | :--- |
| Template is a copy-and-edit folder | `manang plugin new <name> --kind command\|adapter\|agent-tool\|editor` (a Rust CLI, `cargo install`able, in `sdk/`) | M |
| WIT bindings hand-wired | `manang-plugin-sdk` crate: generated bindings, `export_plugin!` / `export_adapter!` macros, typed `Payload` helpers (serde), `TextEdit` builders, test fakes | M |
| Build script is bash | `manang plugin build` (wraps cargo + componentize, validates manifest against the host's own parser) | S |
| Dev loop | `manang plugin dev <dir>`: build on save, copy into a dev plugins dir, host hot reload already swaps atomically; stream plugin logs and manifest errors to the Output panel | S |
| Testing | `manang-plugin-test` harness: boots a real kernel with the plugin, fake `buffer`/`fs`/`http`, fixture runner; for adapters a golden-file runner (`*.in` -> spans, `*.cmd` -> edits) plus the round-trip proptest above, shipped as a library | M |
| Docs | Docs site generated from `docs/plugins.md` plus per-contribution-point pages; the WIT files are the reference | M |
| Examples | (a) `visual-markdown` adapter, (b) `asciidoc` toy adapter proving the claim, (c) command+menu+settings plugin, (d) agent-tool plugin, (e) word-count | M |
| Publishing | signing (ADR 0003 follow-up), registry index (a git repo of manifests + hashes), gallery view | L, later |

The winning claim is demonstrable: *"add a format in an afternoon"* with
`manang plugin new --kind adapter`, a ~300-line parser, and the visual editor,
outline, comments and co-editing working immediately.

## 6. Beta slice, packages, risks

### Interaction with other systems

* **Live co-editing:** edits are text edits on the shared rope/Loro text, so
  there is no model to sync; remote changes arrive as CM transactions (the
  `fromKernel` path) and decorations map through them identically. Remote
  carets render as in `src/lib/editor/live/remote.ts`. This is the decisive
  advantage over a ProseMirror document (which would need a second CRDT
  binding).
* **Figure studio:** `image`/`includegraphics` spans render the saved SVG;
  "Insert figure" creates a `.figure.json` plus `.svg` and returns a text edit
  that inserts the reference; double-click opens the studio.
* **Comments:** anchored to CRDT cursors (stable under edits), stored beside
  the document (sidecar `.manang/comments.json`) so source stays clean;
  highlighted via `Decoration.mark`. docx comments are native `comments.xml`
  and use the same view contribution.

### Work packages (agent-weeks; disjoint files)

| WP | Content | Files | Effort |
| :--- | :--- | :--- | :--- |
| 1 | `manang:format` WIT, `format-adapters`/`editors`/`toolbars` manifest parsing and registries, ADR addendum for sessions | `crates/manang-kernel`, `crates/manang-plugins`, `wit/`, `docs/adr` | 1 wk |
| 2 | `manang-visual` crate: native adapter trait, span-table encoding, session host, binary IPC command, criterion + latency tests | `crates/manang-visual` | 1 wk |
| 3 | Adapters: Markdown (offset events, frontmatter, math, fences) and Typst (`typst-syntax`) incl. command edit plans and proptests | `crates/manang-markdown`, `crates/manang-typst` (adapter modules only) | 1.5 wk |
| 4 | CM6 visual extension: table `StateField`, decorations, cursor reveal, atomic ranges, widgets (heading, emphasis, list, link, math SVG, image), toolbar, mode toggle | `src/lib/editor/visual/` | 1.5 wk |
| 5 | `manang plugin new/build/dev`, SDK crate, adapter example | `sdk/` | 1 wk |
| later | LaTeX adapter (hand parser, macro hints) 1.5 wk; tables with cell edit 1 wk; citations/figure insert polish; docx editor plugin (platform-v2 phase 4) 6-10 wk | | |

WP 1-3 are independent of 4 once the span-table shape is frozen (day 2). Total
about 4 wall-clock weeks with 2-3 parallel agents. Beta acceptance: open a
Markdown and a Typst file, toggle Visual, see headings, emphasis, lists,
links, math and images in place, use the toolbar, undo works, git diff shows
only the intended text, 8 ms p99 keystroke on a 1 MB file, nothing rendered
when the selection is inside a construct.

### Risks

1. **Staleness / flicker** of async span tables (mitigated by mapping,
   revision tags, escape hatch of wasm-in-webview). Measure first.
2. **Stateful sessions** change the plugin security story (memory growth,
   long-lived instances). Hard caps and rebuild-on-trap.
3. **LaTeX macros**: a visual mode that silently misrenders custom commands
   is worse than none; opaque-by-default and visible "source block" chips.
4. **Scope creep to docx**: a WYSIWYG model, pagination and track changes are
   months; keep it a separate plugin and phase.
5. **Typst semantics**: Typst's scripting means a heading may come from a
   function; the adapter must treat unevaluated code as opaque and never claim
   structure it cannot see (use the *syntax* tree, not evaluated content).
6. **Overleaf licence**: AGPL; design reuse only.
7. **Plugin UI isolation** spike (platform-v2 3.4) gates `custom` editors, not
   the built-in visual editor.

### Decisions for the owner

1. Approve the principle: text formats = source-preserving decorations, docx =
   separate model plugin (no Pandoc-style round trip anywhere in the product).
2. Approve the ADR 0003 addendum for stateful adapter sessions, or choose the
   stateless viewport-slice design and accept weaker third-party performance.
3. LaTeX parser: own hand parser (recommended; wasm-clean, MIT) versus
   vendored tree-sitter grammar (faster start, C dependency).
4. Beta scope: Markdown + Typst only, or hold Beta for LaTeX.
5. Visual mode default: opt-in toggle (recommended) or default for Markdown.
6. Priority of the developer experience (`manang plugin new`, SDK, test
   harness) against product features: the owner's "scaffolding is our winning
   thing" implies WP 5 should be in the Beta, not after it.
