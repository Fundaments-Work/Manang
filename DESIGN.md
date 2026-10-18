# DESIGN.md: the Manang workbench

**Design system authority:** [Fundaments.work](https://fundaments.work/brand/) ("square, quiet, and set in two weights").
**Scope:** the desktop shell: layout, surfaces, states, interaction and accessibility. Engine behaviour lives in the Rust crates; this document describes only what the shell presents and how it is operated.

## 1. What it is

Manang is a workbench for writing: a familiar editor layout (activity bar, side panel, tabs, editor with live preview, bottom panel, status bar) with a geometric identity of its own. It should feel as immediate as VS Code or Zed to someone who has used either, and look like nothing else.

Three rules shape every decision:

1. **The shell is thin.** Everything real (files, git, compile, diagnostics, plugins) comes from the Rust kernel through `kernel.run` / `kernel.runBinary`. The shell renders and dispatches.
2. **One signal per frame.** Acid lime marks *where you are*: the active tab's top rule, the open activity icon's edge, the selected slide's edge, the focus ring, the selection. It is never decoration and never used for a button fill.
3. **Square, quiet, tonal.** Radius 0, 1px hairlines, depth by tonal step only.

## 2. Tokens (unchanged, in `src/app.css`)

| Token | Dark | Use |
| --- | --- | --- |
| `--mng-bg` | `#141416` | base, editor |
| `--mng-surface` / `-hi` / `-muted` | `#1c1c20` / `#28282c` / `#353539` | side panel, tab strip, status bar / hover, selected row / reserved |
| `--mng-line` | `#3d3d42` | every hairline (1px) |
| `--mng-text` / `-60` / `-muted` | `#e8e6e1` / `#9a9a9a` / `#7a7a7a` | primary / secondary / hints and disabled (never body text) |
| `--mng-accent` | `#c6f135` | the signal above |
| `--mng-math` | `#b3d17a` | sage, sparingly: math in the editor, search match tint |
| paper | `#faf9f7` / `#1c1c1c` | the rendered page and slides; identical in every theme |

Type: DM Sans 300 and 400, DM Mono 400, nothing else. Scale: 11px (meta, status, mono hints), 12px (UI), 13px (editor, forms), 15px (wordmark, dialog titles), 28px (welcome). Hierarchy comes from size and colour, never a third weight. Sentence case everywhere; no tracked capitals, no eyebrow labels. Spacing is the 4px scale. Motion is limited to the palette fade and the zen hint, both responses to an action; `prefers-reduced-motion` removes it.

Shared controls live in `app.css` as `.mng-btn`, `.mng-icon-btn`, `.mng-field`, `.mng-kbd`, `.mng-panel-head`. Icons are a 16-unit inline set (`ui/Icon.svelte`): squares, rules and diagonals, no circles.

## 3. Layout

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [⩓] Manang  Project / dir / file.typ   [ Search commands   Ctrl+K ]  Split Editor Preview  Export PDF  Zen │  top bar 40px
├────┬───────────────┬─┬───────────────────────────────────────────┬────────────┤
│ ▌  │ Files  + ▢ ↻ ⇕│ │ main.typ ×  results.typ ●        Text│Slides│            │  tab strip 36px
│ F  │ [Filter files]│ ├───────────────────────────┬─┬─────────────┤            │
│ S  │ ▸ figures     │ │ 1  = Title                │ │ ↑ ↓ Page 1/2 │            │
│ Fg │ ▾ sections   ▪│ │ 2  ...                    │ │ − Fit +  ↻ ⤓ │            │
│ R  │     results M │ │                           │ │  ┌────────┐  │            │
│ P  │   main.typ    │ │                           │ │  │ paper  │  │            │
│    │   notes.md  U │ ├───────────────────────────┴─┴─────────────┤            │
│    │               │ │ Problems 2 │ Output                      × │  bottom panel (Ctrl+J)
│ ⚙  │               │ │ ⊠ Unclosed math delimiter       Ln 3, Col 6│            │
├────┴───────────────┴─┴───────────────────────────────────────────────────────┤
│ ⑂ main*  ⊠ 2 errors   [notice]            Ln 3, Col 6  67 words  Typst  Kernel│  status bar 24px
└──────────────────────────────────────────────────────────────────────────────┘
```

* **Activity bar** (48px): Files, Search, Figures, References, Plugins; Settings at the bottom. Clicking an icon opens its side panel; clicking the open one collapses it. The Files icon carries the git change count. Project-bound views are dimmed until a folder is open.
* **Side panel** (200 to 480px, default 264, draggable, double-click resets). Under 800px it overlays the editor instead of squeezing it; opening a file or pressing Esc dismisses it.
* **Tab strip + editor area.** Tabs hold a document, a figure, or a document shown as slides. The editor and the preview split with a draggable divider (25 to 75%).
* **Bottom panel** (96 to 560px): Problems and Output.
* **Zen** removes every piece of chrome and leaves the editor as a 680px reading column (no gutter, 15px). Esc leaves.

Narrow behaviour: at 1024px everything fits; under 800px the side panel overlays and split falls back to the editor alone (an explicit "Preview" choice is kept); at the 640px minimum the top bar reduces the palette field and the three actions to icons, and nothing scrolls sideways.

## 4. Vocabulary

| Old | Now |
| --- | --- |
| Library Shelf | **Files** (`FilesPanel`, `workspace.svelte.ts`) |
| Instrument Shelf | **Plugins** (`PluginsPanel`, `workbench/plugins.ts`) |
| Whispers, Margin Whispers | **Problems** (`BottomPanel`, `workbench/problems.ts`, gutter markers) |
| Desk | **workspace** and **editor** (`Workbench`, `EditorArea`, `EditorPane`) |
| Canvas, ink | **Figure editor** (`FigureEditor`, `lib/figure`, `figure.svelte.ts`) |
| Presentation Studio | **Slides** (a view of a `.typ` document) |
| Curated Lab | removed |

## 5. Surfaces and their states

**Welcome** (no project). Open folder (by path, because the native picker needs the dialog plugin; recents are the quick way back), New project (name, Typst / LaTeX / Markdown, location), recent projects from `project.recent`. A missing folder is listed, disabled, as "Folder not found". A non-project folder says to choose the folder containing `manang.toml` or create a project.

**Files.** ARIA tree, lazy per folder, git badges (M, A, D, R, U, !) from `project.git-status`, a dot on folders containing changes, a square on files with unsaved edits. Filter is fuzzy across all paths. New file, new folder (kept alive by a hidden `.gitkeep`), rename (F2) and delete (to trash) happen inline or from the context menu. States: loading, empty folder, no filter matches.

**Search.** File names (fuzzy) and text (open documents with unsaved edits, other text files read via `project.read-file`, first 80). Results jump to line and column.

**Figures.** The project's `*.figure.json` files and New figure. Opening one puts the figure editor in a tab.

**References.** Entries from `.bib` files, word-AND filter, count. Choosing one inserts a citation in the active document's syntax (`@key`, `\cite{key}`, `[@key]`). Parsing is a client-side stub behind `BibSource`; the citations plugin replaces it.

**Plugins.** The live `kernel.commands()` grouped by plugin, each expandable to its commands. The kernel does not report tier or permissions yet, so none are shown. Empty state explains the kernel is unreachable; a note says third-party installation is not available yet.

**Settings.** Theme and layout reset. Persisted in browser storage until the settings plugin lands.

**Tabs.** Dirty square that becomes a close mark on hover, close button, middle-click, drag to reorder, context menu (close, close others, close to the right, show as slides, copy path), overflow scrolls. Same-named files get their folder as a suffix.

**Preview.** Real `typst.render` SVG. Toolbar: previous and next page, "Page n of N", zoom out, zoom (Fit, 50 to 200%), zoom in, refresh, export PDF. States: compiling (last page stays), ready, failed (last good page plus a banner linking to Problems), no compiler for this engine, compiler missing. Compile runs 250ms after typing stops and sends every unsaved buffer.

**Problems and Output.** Problems are `typst.compile` diagnostics grouped by file with severity icon (solid square-cross for error, triangle for warning, so severity never relies on hue), message, hints and `Ln, Col`; click jumps there. The tab label and status bar carry the count. Output is the compile log. The editor gutter shows a solid (error) or hollow (warning) 6px square on the line, with a hover callout.

**Slides.** A `.typ` document viewed as a deck: each heading is a slide. Thumbnail strip (listbox), current slide, Present (F5), full-window presentation (arrows, Space, Esc).

**Figure editor.** One toolbar row in groups (pointer tools, shapes, lines and drawing, text and formulas and Mermaid, history, zoom; Copy embed line and Save at the right), a layers panel on the left (icon, name, lock, visibility; click, Shift-click, F2 rename, drag to reorder), the canvas, and a contextual inspector on the right (the figure when nothing is selected; otherwise Layout, Appearance, Fill, Stroke, Text, Effects, Arrange, each collapsible and remembered). Numeric fields scrub from their labels and show Mixed across a disagreeing selection. Under 860px of studio width the panels become overlays opened from the toolbar. Scenes load and save as JSON through the project plugin.

**Status bar.** Branch (`main*` when dirty), compile state with timing ("Compiled in 9 ms", "Compiling", "2 errors"), a transient notice, caret, words, engine, kernel connection ("Demo kernel" in the browser). Branch and compile state are buttons.

**Command palette.** `Ctrl+K` or `Ctrl+Shift+P` for commands (workbench and plugin, grouped, with real shortcuts; files appear once you type); `Ctrl+P` for go to file. It is a shortcut: every command is reachable elsewhere.

## 6. Keyboard map

| Action | Keys |
| --- | --- |
| Command palette / go to file | Ctrl+K or Ctrl+Shift+P / Ctrl+P |
| Files, Search, Plugins, Settings | Ctrl+Shift+E, Ctrl+Shift+F, Ctrl+Shift+X, Ctrl+, |
| Toggle side panel / bottom panel / Problems | Ctrl+B / Ctrl+J / Ctrl+Shift+M |
| Next / previous tab | Ctrl+Tab or Ctrl+PgDn / Ctrl+Shift+Tab or Ctrl+PgUp |
| Tab 1 to 8, last tab | Ctrl+1 to Ctrl+8, Ctrl+9 |
| Close tab, save, save all, new file | Ctrl+W, Ctrl+S, Ctrl+Shift+S, Ctrl+N |
| Cycle preview layout, export PDF, zen | Ctrl+Shift+V, Ctrl+E, Ctrl+\ |
| Present slides | F5 |

Cmd replaces Ctrl on macOS. Shortcuts are data (`commands/workbench.ts`); the handler and the palette read the same list. In a browser some chords (Ctrl+W, Ctrl+Tab) belong to the browser; they work in the desktop app.

## 7. Accessibility

* Roles: `tablist`/`tab`/`tabpanel` (automatic activation, arrows, Home, End, Delete, Shift+F10), `tree`/`treeitem` with level, set size and position (arrows, Home, End, type-ahead, Enter, F2, Delete), `menu`/`menuitem` (roving focus, Esc returns focus), `dialog`/`alertdialog` with focus restored, `combobox` + `listbox` + `aria-activedescendant` for the palette, `separator` with value attributes for resizers (arrows, Home, End), `toolbar`, `radiogroup`, `log`, live `status` regions.
* Focus is always visible: a 1px lime outline, 1px offset (inset inside scrolling lists).
* State is never colour alone: pressed, selected and severity also change shape, weight of border or text.
* Targets: 28px controls, 48px activity icons. Text meets AA on every surface; `--mng-text-muted` is never used for body text.
* `prefers-reduced-motion` is honoured globally.

## 8. Data, real and mocked

Everything goes through `$lib/kernel`: the Tauri client inside the app, an in-memory dev kernel in a browser (`pnpm dev`, tests). The dev kernel reproduces the editor, project and typst manifests (commands, arguments, result shapes, failure messages) with a demo project, git status, `project.changed` events, and a stand-in compiler that reports unclosed math and brackets, missing includes, figures and citations, and draws pages as SVG. Large artefacts stay `Uint8Array`. Events arrive on `kernel://event` and are ignored when there is no Tauri.

## 9. Persistence

Layout (panel sizes, open panels, preview layout, theme) and session (last project, open tabs and their views) are saved to `localStorage` through `workbench/persist.ts`, every access wrapped in try/catch. The settings plugin will take over; only that module changes.

## 10. Known limits

* Open folder takes a path; a native picker needs the Tauri dialog plugin.
* Export PDF downloads through the webview; saving into the project needs a binary write command.
* The preview target is the active Typst file; pinning a main file for `#include`d chapters is not built.
* Folder listings stop at 5,000 entries per directory.
* LaTeX and Markdown edit and save but have no preview until their plugins exist.
