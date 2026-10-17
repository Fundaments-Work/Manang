# Manang: Feature Gap Analysis

Date: 2026-10-18. Sources: a read-only audit of the code at `main` (cde7b2d)
and research on comparable tools (Overleaf, the Typst web app, VS Code, common
whiteboard tools). Companion to `2026-10-18-platform-v2.md`.

## Summary

The Rust side is far ahead of the UI. Most plugin commands exist and are tested,
but the UI calls only `editor.*`, `project.*` and `typst.*`. Figure, search, git,
settings, keybindings, theme and LaTeX commands are registered but no UI uses
them; Markdown, Citations and Writing tools are not even registered. Integration
is the first job; new features come after.

Legend: **BROKEN** = exists but fails; **UNWIRED** = backend exists, UI does not
use it; **MOCK** = UI fakes it client-side; **MISSING** = nothing exists.

## P0: Make what we have work (integration)

| # | Area | State | What is needed |
| :--- | :--- | :--- | :--- |
| 1 | App start on Wayland | BROKEN | Tectonic's static HarfBuzz crashed GTK/WebKit. Fixed by moving Tectonic into the worker binary only (feature `engine`); verify and ship. |
| 2 | Palette runs plugin commands with no arguments | BROKEN | Only list commands that take no arguments, or collect arguments; hide internal commands. |
| 3 | Search panel | MOCK | Use `search.files`, `search.text` (regex, case, whole word, include/exclude), `search.replace-preview/apply`, `search.cancel`. Today: TS literal search capped at 80 files. |
| 4 | Git | UNWIRED | No Git view at all. Need a Source Control view: status, stage/unstage, discard (confirm), commit, branches, log, diff view, gutter change markers. |
| 5 | Settings | MOCK | A real Settings page from `settings.schema` (user and project scope, search, reset), keybinding editor (`keybindings.*`), theme picker (`theme.*`). Today: one theme toggle in localStorage. |
| 6 | References | MOCK | Use `citations.*` (load, search, format, verify, extract-keys). Register the plugin. Citation autocomplete in the editor. |
| 7 | Writing tools | NOT MERGED | Merge `manang-writing`; register it; wire spellcheck squiggles, prose lint into Problems, Format document, symbol picker, snippets, word count in the status bar. |
| 8 | Markdown | NOT REGISTERED | Register `manang-markdown`; Markdown preview via `markdown.render-html`; outline. |
| 9 | LaTeX | UNWIRED | Preview never compiles. Wire `latex.compile` and `latex.export-pdf`, a PDF viewer, Problems from TeX logs, SyncTeX jump. Ship the worker sidecar. LaTeX highlighting is wrong (uses the Typst grammar); BibTeX has none. |
| 10 | Figure editor | BROKEN | Shapes cannot be moved; formula text fixed to one string; "Copy embed line" points to an SVG that is never written; no use of `figure.*`. (Full whiteboard is P2.) |
| 11 | Plugins view | UNWIRED | Plugin manager has no IPC: install, enable/disable, permissions, scope, errors; UI ignores `plugins.*` events. |
| 12 | Editor completions | MISSING | Autocomplete is on but has no sources. Needs Typst completions (typst-ide in the Typst plugin: a backend gap), citation keys, labels, snippets, symbols. |
| 13 | Native dialogs and files | MISSING | Open folder takes a typed path; Export PDF downloads through the webview. Need the Tauri dialog plugin (app-level capability) and a binary write command. |
| 14 | Editor plugin unused | UNWIRED | `editor.undo/redo/save/replace` unused; CodeMirror history is the source of truth. Decide one owner of undo and saving. |

## P1: A complete everyday experience

Compared with Overleaf, the Typst web app and VS Code.

**Home and projects**
- Project home separate from the editor: grid and list, search, sort, tags,
  pinned, archive, trash, first-page thumbnails (Overleaf-style).
- Home button and project switcher inside the editor; recent files.
- Templates gallery (blank per engine; IEEE, ACM, Nature, arXiv, thesis, letter,
  slides); import from zip, folder, git clone.

**Writing**
- Outline panel and breadcrumbs; go to heading or label.
- Citation, label and reference autocomplete; symbol picker; snippets with tab
  stops; spellcheck with language choice and personal dictionary.
- Format document; word and character counts; reading time.
- Image and asset handling: drag and drop or paste images into the editor,
  asset previews (images, PDF), insert-figure dialog.

**Compile and preview**
- Preview for every engine (Typst, LaTeX, Markdown), synced scrolling, click in
  preview to jump to source and back (Typst `jump`, LaTeX SyncTeX).
- Compile log (Output), error count in the status bar, stop compile, choose main
  file per project (Overleaf has this), compile on save or on change.
- Export: PDF, HTML, bundle; save dialog.

**History and review**
- Local history snapshots (works without git) and a git timeline with diffs and
  restore. Overleaf keeps full history on paid plans and 24 hours on free.
- Comments and track changes (Overleaf, Typst Pro): plan with collaboration.

**Integrations**
- Zotero and Mendeley sync and DOI import (Overleaf and Typst Pro offer these).
- Git remote sync (push, pull) from the UI.

**App quality**
- Toast notifications, progress for long tasks, onboarding, help and shortcut
  reference, About and licences (the SCOWL dictionary licence must be shown).
- Autosave settings, crash recovery and hot exit for unsaved work.
- Accessibility pass; reduced motion; zoom; later i18n.

## P2: Platform v2 (see the platform plan)

- Plugin contribution points (document types, editors, views, importers and
  exporters, settings), system and document scopes, sandboxed plugin UI.
- Whiteboard-grade figure editor as a document plugin.
- `.docx` editor and preview as a plugin, the acceptance test of the platform.
- Collaboration (CRDT), comments and track changes.
- Packaging, signing, auto-update, CI and performance gates, security review.

## Notes

- Research sources: Overleaf help and blog (track changes, comments, history,
  new editor layout), the Typst web app docs and 0.15 release notes
  (autocomplete, previews, Pro features: comments, git sync, Zotero sync).
  Whiteboard feature expectations come from common practice, not a verified
  feature matrix.
- The P0 list is ordered so that each item can be verified in the running app.
