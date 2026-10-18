# LibrePaper (librepaper.org): comparison and what we do about it

Date: 2026-10-18. Sources: https://librepaper.org/, https://github.com/LibrePaper/librepaper,
https://github.com/LibrePaper/wasm-latex, https://github.com/LibrePaper (org page). Vendor
statements; unverified items marked.

## What it is

A free, open source (MIT) **web platform** to "read, write, and review together":
Rust server plus Postgres, deployed as one binary (self-hostable), with a CLI
(`login`, `list`, `export`, `admin`, `mcp`) and a desktop folder in the repo (the
site only says the desktop app keeps scheduled ZIP backups; whether it authors
offline is unverified). Copyright holder: Vincent Arel-Bundock (single author;
the repo has AGENTS.md and CLAUDE.md, so it is built with AI agents). 42 stars,
2,626 commits on main, 15 open issues, active (updated 2026-10-18). Public host:
50 MB per user, 30 uploads per hour; "everything is free, no feature gated".

Formats: HTML, LaTeX, Typst, Markdown, Quarto, Calepin (computational
notebooks: Quarto and Calepin). BibTeX and Zotero. Live preview for Markdown,
HTML, Typst and LaTeX.

Collaboration: simultaneous editing with named carets and personal undo,
highlights and threaded comments, **track changes** (accept or reject each
edit), "granular and infinite" history, sharing with reader/commenter/editor
roles by link, chat with collaborators and AI agents. AI: bring your own agent
through MCP (commercial or local models); a sidebar chat and pre-specified
tasks on a selected passage; "does not need your keys".

Renderers: separate MIT repos compiled to WebAssembly: `wasm-latex` (pdfTeX,
XeTeX, dvipdfm, BibTeX, BibTeX8, Biber, makeindex, experimental LaTeXML; the
vendored Biber build keeps AGPL-3.0; 41 commits, 1 star), `wasm-typst`,
`wasm-markdown`, `wasm-bibliography`, `wasm-helpers`.

## Where it is ahead of us

- Zero install: everything runs in the browser, including LaTeX (no TeX
  distribution needed). Our LaTeX needs a system TeX or the optional Tectonic.
- Review by link: a non-writer (advisor, co-author) opens a link and
  comments. We have no hosted surface.
- Real-time editing, comments and track changes are shipped now.
- Computational notebooks (Quarto, Calepin): we have none.
- A large head start in commits, from a developer using agents heavily.

## Where we are different, and ahead

- **Local-first desktop:** files on your disk, git-native, works offline, no
  server, nothing uploaded. Theirs is server-centred.
- **Figure studio:** an Excalidraw-class vector editor with bound arrows,
  Mermaid to shapes, Typst/LaTeX formulas, and agent tools; they show none.
- **Native speed:** Typst compiles in single-digit to tens of ms, a PDF viewer
  with selection, find and links; incremental, in-process.
- **AI with control:** bring-your-own key including local models, a visible
  context tray, proposals as diffs the user accepts, audit log, per-project
  off switch; plus MCP later.
- **Platform:** sandboxed WASM plugins with a consent-based plugin manager,
  contextual Settings, a policy layer for organisations (planned).
- **Data safety and history:** Save a version and restore, crash recovery,
  versioned formats (in progress).

## Positioning

LibrePaper is "a free Overleaf alternative you can host". Manang is "a
local-first scientific authoring studio": the tool for people and
organisations whose documents must stay on their own machines, who work
offline, who draw figures inside the paper, and who want AI under their own
control. Overlap is collaboration; do not try to out-host them. Win on the
quality of the local experience, figures, speed, AI control and trust.

## What to do

1. Stay the course: private beta focused on the differentiators (stability,
   figure studio, AI with a context tray, local-first collaboration).
2. **Spike: a WebAssembly TeX as the "no TeX installed" backend.** Their
   `wasm-latex` (MIT, with an AGPL Biber build to avoid or isolate) is a
   candidate for running pdfTeX/XeTeX inside our Wasmtime host (sandboxed,
   consistent across platforms) and for the future web version. Questions:
   WASI target or browser-only, package bundle size and on-demand download,
   SyncTeX, multi-pass builds, licence of every component, speed against system
   TeX. This removes our weakest onboarding step.
3. **Review by link** as a later feature on the live-editing relay: a read and
   comment role for people who do not install anything (needs the relay and a
   small web viewer; ties into comments and review mode).
4. **Computational documents** decision for the owner: Quarto (`.qmd`) through
   the user's installed `quarto`, executable code blocks, Jupyter kernels.
   Scientific authoring often needs it; scope it as a research spike first.
5. Interoperate rather than compete on storage: both use plain project folders;
   make sure Manang opens a LibrePaper export cleanly and vice versa.
6. Move fast on what only we can do (figure studio, AI agent, local-first
   safety); their pace is high, so depth and stability are the moat.
