# ADR 0015: Visual editor - format adapters, span tables, stateful sessions

Status: accepted (owner approved the six recommended decisions), 2026-10-18.
Wire format **frozen at version 1** (section 3). Additive changes only: new
kinds, new commands, new optional JSON fields. A layout change bumps
`version` and the magic.
Evidence: `docs/research/2026-10-18-visual-editor-and-document-plugins.md`.
Scope: `crates/manang-visual` (core, wasm32-capable), `crates/manang-visual-plugin`
(kernel glue), adapter modules in `crates/manang-markdown` and `crates/manang-typst`,
manifest contribution points in `crates/manang-kernel`, `wit/format.wit`,
`src/lib/editor/visual/` (the CodeMirror extension, a different agent).

## Decisions

1. **Source stays the only truth** for text formats. Visual mode is CodeMirror
   decorations over the real source, driven by a span table that Rust computes.
   Every toolbar action is an **edit plan of ordinary text edits**; the web
   view applies it as one CodeMirror transaction, so undo, the Loro CRDT, git
   and remote carets see plain text. Nothing is serialised from a model.
2. **One `visual` plugin, per-format adapters.** The plugin never mentions a
   format. Format plugins contribute `format-adapters`; the visual plugin
   resolves toolbar commands through the active document's adapter.
3. **Visual is an opt-in toggle.** Source mode stays the default. A format
   without an adapter has no toggle.
4. **Stateful adapter sessions** (addendum to ADR 0003, section 5 below).
5. **Markdown and Typst first** (native first-party adapters), LaTeX next with
   an own hand-written parser (wasm-clean, MIT), docx as a separate model plugin.
6. The plugin SDK and `manang plugin new` are part of the beta (separate work).

## 1. Units

* **Everything on the wire is UTF-16 code units** (CodeMirror's unit). The web
  view never converts. The span table, `from`/`to` arguments, edit changes and
  edit plans all use them.
* Adapters, being Rust parsers, work in **UTF-8 byte offsets** internally and
  through the native trait. The session host owns a rope and converts at the
  boundary, so third-party adapters never see UTF-16.
* A *change list* (`edit` input, `edit-plan` output) uses CodeMirror's
  `ChangeSpec` semantics: entries `{from, to, insert}` sorted ascending,
  non-overlapping, **all positions in the document before the batch**. A plan
  can therefore be passed to `view.dispatch({ changes })` unchanged.

## 2. Architecture

```
CM6 transaction ──visual.edit(changes)──▶ session (rope + adapter state)
       ▲                                       │ incremental reparse
       │  decorations mapped through ChangeSet │
       └─────── visual.blocks(viewport) ◀──────┘  binary span table
toolbar ──visual.run-command──▶ adapter ──▶ edit plan ──▶ one CM6 transaction
```

* The session mirrors **the CodeMirror document** (user, remote and undo
  transactions alike all go through `visual.edit`). It is *not* a second copy
  of the kernel buffer; it is a derived index of what the user sees. If the
  copies drift, the host detects it (`len` check) and the web view resyncs with
  `visual.reset`.
* **Keystroke path (8 ms p99).** CM maps the existing decorations through the
  `ChangeSet` synchronously (frame renders at once). `visual.edit` and then
  `visual.blocks` run off the typing frame, results are tagged with the session
  revision and dropped by the web view if older than its local state. A widget
  can be stale for a frame; it is never wrongly attached.
* Heavy work runs on a private Rayon pool inside `manang-visual-plugin`;
  Tokio only carries the IPC call.

## 3. Span table (binary, frozen)

Returned by `visual.blocks` as raw bytes (`kernel_invoke_binary`, which hands
Tauri a `tauri::ipc::Response`). Little-endian, all offsets in the table are
UTF-16 code units, all fields 4-byte aligned.

Header, 40 bytes:

| off | type | field |
| :-- | :-- | :-- |
| 0 | 4 bytes | magic `4D 56 53 31` (`"MVS1"`) |
| 4 | u16 | `version` = 1 |
| 6 | u16 | `flags`: bit 0 `truncated` (span cap `MAX_SPANS` = 65 536 hit; ask a smaller viewport) |
| 8 | u64 | session `revision` (the revision the table describes) |
| 16 | u32 | `from`: requested viewport start (clamped to the document) |
| 20 | u32 | `to`: requested viewport end |
| 24 | u32 | `doc_len` in UTF-16 units at `revision` |
| 28 | u32 | `span_count` |
| 32 | u32 | `string_count` |
| 36 | u32 | `string_bytes` |

Then `span_count` records of 16 bytes (`Uint32Array` view at byte 40):

| word | content |
| :-- | :-- |
| 0 | `start` (u32) |
| 1 | `end` (u32), `end > start` |
| 2 | `kind` u16 (low half) \| `layer` u8 (bits 16-23) \| `aux` u8 (bits 24-31) |
| 3 | `attr`: index into the string table, or `0xFFFF_FFFF` for none |

Then `string_count` u32 **cumulative end offsets** into the string bytes, then
`string_bytes` bytes of concatenated UTF-8 strings (string `i` spans
`ends[i-1]..ends[i]`, with `ends[-1] = 0`). Total size is exact; nothing follows.

**Invariants the host enforces for every adapter** (proptested):

* `0 <= start < end <= doc_len`.
* Records are sorted by `start` ascending, then `end` descending (ancestors
  before descendants), and equal ranges keep adapter order (parent first).
* The family is **laminar**: two spans are nested or disjoint, never crossing.
  `layer` is the number of enclosing spans (0 = outermost). Therefore spans of
  one layer are disjoint (touching allowed) and sorted.
* A slice contains every span that intersects `[from, to)` (a zero-width
  viewport counts as `[from, from+1)`), **whole, even if it starts before
  `from`**, plus all its ancestors. It never contains non-intersecting
  descendants.
* The table is a full, idempotent description of the viewport. There is no
  patch format: after an edit, the web view re-requests the viewport when the
  `dirty` range returned by `visual.edit` intersects it. This is the "delta":
  only dirty viewports cost a request, and the server only reparses the dirty
  region.

**Kinds** (u16; 0-255 reserved for first-party vocabulary, 1024+ for
adapter-defined kinds, rendered as an opaque chip labelled by the name the
adapter lists in `kinds`):

| id | name | aux | attr |
| :-- | :-- | :-- | :-- |
| 0 | `opaque` | | optional label (`set`, `show`, `html`, ...) |
| 1 | `heading` | level 1-6 | |
| 2 | `strong` | | |
| 3 | `emph` | | |
| 4 | `code` (inline raw) | | |
| 5 | `code-block` | | language |
| 6 | `list-item` | 0 bullet, 1 ordered, 2 task open, 3 task done | |
| 7 | `link` | | destination |
| 8 | `image` | 1 = figure | source path |
| 9 | `quote` | | |
| 10 | `math` | 0 inline, 1 block | `latex` or `typst` |
| 11 | `frontmatter` | | |
| 12 | `table` | | (rendered later) |
| 13 | `rule` | | |
| 14 | `ref` | | key |
| 15 | `label` | | key |
| 16 | `comment` | | |
| 17 | `marker` | | |
| 18 | `strike` | | |
| 19 | `cite` | | keys, comma separated (reserved) |

A `marker` span is syntax the renderer may hide when the selection is not in
its parent (`**`, `#` plus space, `[` and `](url)`, fences, list markers).
Markers are children of the construct they belong to. Content that must not be
decorated inside (math, raw, opaque) is simply a leaf: the web view never
descends into `math`, `code`, `code-block`, `opaque`, `frontmatter` children.

## 4. IPC commands (frozen)

Kernel commands of the first-party `visual` plugin, called through the
existing `kernel_invoke` (JSON) and `kernel_invoke_binary` (binary) Tauri
commands. Errors use the shell's `[code] message` shape with codes
`visual.no-session`, `visual.no-adapter`, `visual.bad-args`, `visual.desync`,
`visual.unsupported`, `visual.too-large`, `visual.failed`.

| command | args | result |
| :-- | :-- | :-- |
| `visual.adapters` | none | `[{id, title, extensions, kinds[], commands[]}]` |
| `visual.open` | `{doc?: number, text?: string, adapter?: string, path?: string}` (text from the editor document `doc`, or inline `text`; adapter by id, else by `path` extension) | `{session, adapter, revision: 0, len}` |
| `visual.reset` | `{session, doc?, text?}` | `{session, revision, len}` (revision continues, grows) |
| `visual.close` | `{session}` | null |
| `visual.edit` | `{session, changes: [{from, to, insert}], len}`; `len` = document length after the changes | `{revision, len, dirty: {from, to} \| null}`; `dirty` in new coordinates; `visual.desync` if `len` disagrees |
| `visual.blocks` | `{session, from, to}` | **binary** span table |
| `visual.outline` | `{session}` | `[{level, title, from, to}]` |
| `visual.command-state` | `{session, from, to}` | `[{command, enabled, active}]` |
| `visual.run-command` | `{session, command, from, to, args?: string}` | `{changes: [{from, to, insert}], selection: {from, to} \| null}`; `selection` is in the document *after* the changes |

Adapter commands (ids without a plugin prefix; the toolbar maps
`visual.bold` to the adapter's `bold`): `bold`, `italic`, `code`, `strike`,
`heading` (args `"0"`-`"6"`, 0 = plain paragraph), `list` (args `bullet` |
`ordered`), `quote`, `link` (args = url), `image` (args = path), `figure`
(args = path), `math` (args `inline` | `block`), `rule`. An adapter lists only
what it implements; `command-state.enabled` is false elsewhere.

Sessions are per open editor view, at most 64, each document at most 64 MiB.
Calls on one session are serialised; calls on different sessions are parallel.

## 5. Manifest contribution points

Added to `Contributes`, parsed and registered by the kernel (registries on
`Kernel`: `format_adapters()`, `editors()`, `toolbars()`):

```toml
[[contributes.format-adapters]]
id = "markdown"                      # kebab-case, unique across plugins
title = "Markdown"
extensions = ["md", "markdown"]      # lowercase, no dot
document-type = "markdown"           # optional, names a document type
kinds = ["heading", "strong", "emph", "list-item", "link", "image"]
commands = ["bold", "italic", "heading", "list", "link"]

[[contributes.editors]]
id = "visual"
kind = "visual"                      # source | visual | custom
document-types = ["*"]               # or format-adapter ids / extensions
title = "Visual"
priority = "optional"                # optional | default; visual never replaces source by default
# entry = "ui/editor.html"           # custom editors only

[[contributes.toolbars]]
editor = "visual"
[[contributes.toolbars.items]]
command = "visual.bold"              # must be declared by the same manifest
icon = "bold"
when = "adapter.has(bold)"           # optional; evaluated by the shell
```

## 6. Addendum to ADR 0003: stateful adapter sessions

ADR 0003 makes every call stateless (fresh `Store`, nothing survives). That is
right for commands and hooks and wrong for incremental parsing. Addendum:

* **Commands and hooks stay stateless** as decided.
* A plugin that contributes a `format-adapter` may export the `session`
  resource of `manang:format/adapter` (`wit/format.wit`). The host keeps **one
  `Store` and instance per open session** alive between calls and routes the
  resource's methods to it. Nothing else may be stateful.
* **Limits per session**: memory cap (`Limits::memory_bytes`, default 128 MiB,
  hard), the existing per-call epoch deadline (default 50 ms for adapter
  calls; `open` and `reset` 250 ms), a maximum idle lifetime, and 64 sessions
  per plugin. No fuel carried over; each call re-arms its deadline.
* **Rebuild on failure.** On a trap, deadline, cap breach or hot reload the
  host drops the instance and rebuilds it from the host's rope with `open`; the
  caller sees at most one `visual.failed` and then continues. State is
  *derived*: the host's rope is the truth, so losing an instance loses nothing.
* **Capabilities are unchanged**: an adapter has no filesystem, network or
  buffer capability. It receives text from the host and returns spans and text
  edits; it can never write the buffer (the plan is applied by the web view).
* **Fallback** for adapters that do not export `session`: the host calls the
  stateless `blocks-in-slice` function with a text slice per request. No
  runtime change, adequate for Markdown-sized parsers.
* **First-party native adapters** implement the Rust trait
  `manang_visual::FormatAdapter` directly and bypass the runtime; the trait is
  the reference shape of the WIT interface and is object-safe.

Reference implementation: `wit/format.wit`, `manang_visual::adapter`.

## 7. First-party adapters

* **Markdown** (`manang-markdown::visual`): pulldown-cmark offset events over
  independently reparsed top-level blocks (block index with resync, so an edit
  reparses a few blocks, not the file). Headings, emphasis, strong, strike,
  code, fences, lists (task lists), links, images, blockquotes, math, rules,
  frontmatter, tables (detected, kind only).
* **Typst** (`manang-typst::visual`): `typst-syntax::Source::edit`
  (incremental) and a viewport walk of the syntax tree. Headings, strong, emph,
  raw, lists, links (`#link`), refs, labels, math, `#image`/`#figure`,
  comments; `#set`, `#show`, `#let`, `#import` are `opaque` spans. Only the
  *syntax* tree is used: scripted structure is never claimed.

## Consequences

* The web view is thin: build `Decoration`s from a typed array.
* Co-editing needs no special case: remote changes are CM transactions, so they
  reach the session as `visual.edit`.
* Cost: a second copy of the text (rope + parser state) per open visual
  document. Acceptable (a 1 MB document is a few MB of state).
* Risk: staleness for a frame after an edit; mitigated by mapping and revision
  tags. If measurement shows it is visible, the escape hatch is compiling the
  same adapter crate to wasm for the web view (also needed for the web version).

## Implementation notes (2026-10-18)

* **Crates.** `manang-visual` (core, `cargo check --target wasm32-unknown-unknown`
  clean), `manang-visual-plugin` (kernel glue and the Rayon pool),
  `manang_markdown::visual`, `manang_typst::visual`. The wire format above is
  implemented in `manang-visual/src/table.rs` and decoded again there for tests.
* **Toolbar commands.** `visual.bold`, `visual.italic`, `visual.code`,
  `visual.strike`, `visual.heading`, `visual.list`, `visual.quote`,
  `visual.link`, `visual.image`, `visual.figure`, `visual.math`, `visual.rule`
  are `visual.run-command` with the adapter command fixed; args are
  `{session, from, to, args?}`. The toolbar contribution of the `visual`
  manifest lists them with `when = "adapter.has(<command>)"`.
* **Registries.** `Kernel::format_adapters()`, `editors()`, `toolbars()` and the
  `plugins.contributions` command (`{format_adapters, editors, toolbars}`).
  The first-party editor registers its own `source` editor (`kind = "source"`).
* **Markdown** keeps a list of top-level blocks with spans relative to each
  block. An edit reparses a window of whole lines: the block before the edit
  (and further back while it is open or not preceded by a blank line) up to a
  block boundary that follows a blank line. Frontmatter is detected by the
  adapter (the parser's own option accepts it at any block start). Reference
  style links and images (`[a][r]`, `[r]`) are left as source text because
  their meaning depends on definitions the window cannot see. Tables are
  reported as a `table` leaf.
* **Markdown and link definitions.** A definition also decides whether an
  *inline* link parses: in `[[r]: http://x\na](u)` the inner `[r]` closes on
  its own and takes the outer `](u)` with it as soon as any `[r]:` definition
  exists in the document. So an edit that can add, remove or invalidate a
  definition (any line it touches, plus one on each side, or any inserted line,
  is up to three spaces and a `[`) re-reads the whole document, as an edit that
  can create or destroy frontmatter already does. Prose never takes that path.
* **Typst** relies on `Source::edit`, which rescans the text after the edit to
  rebuild line starts (about 1.6 ms median on a 1 MB document, typst-syntax
  internals). For a document with a syntax error somewhere, Typst's incremental
  tree may keep an older recovery; for valid markup it equals a fresh parse
  (proptested).
* **Measured** (criterion, `cargo bench -p manang-visual`, 1 MB document, 40 KB
  viewport, one edit plus the encoded span table): Markdown p50 0.29 ms, p99
  0.39 ms; Typst p50 2.8 ms, p99 4.2 ms. Opening a 1 MB document (full parse):
  Markdown 21 ms, Typst 52 ms, on the Rayon pool.
* **Not done yet:** the WASM host binding for `manang:format` (native adapters
  only), the stateless `spans-in-slice` fallback adapter, `visual.blocks`
  streaming straight from the encoder into the IPC buffer (one copy of a
  table of tens of KB remains in `kernel_invoke_binary`), table cells, citations.
