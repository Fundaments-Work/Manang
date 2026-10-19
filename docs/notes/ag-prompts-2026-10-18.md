# Antigravity tasks, 2026-10-18

Three self-contained tasks. Run each in its own worktree and branch; they do
not overlap with each other or with the Claude agents now working on the PDF
viewer, the figure studio, the menu bar and contextual Settings. Paste one
task (its whole section) into Antigravity.

Common rules for every task (also read AGENTS.md v1.3.0; it is binding):

- Create your worktree from the current `main` on the SSD:
  `cd ~/Development/Fundaments/Manang && git fetch && git worktree add -b antigravity/<topic> ../Manang-ag-<topic> main`
  then `cd ../Manang-ag-<topic> && pnpm install --frozen-lockfile --offline`.
- Commit early and often on your branch with Conventional Commits; never
  commit to `main`, never push, never merge. Report your branch and commits
  when done, and append a short summary to `docs/notes/ag-progress.md`.
- Rust: `export CARGO_TARGET_DIR=/home/sapiens/Development/Fundaments/Manang/target`,
  always `-p <crate>`, never workspace-wide, never `cargo clean`. Gates:
  `cargo fmt --all -- --check`, `cargo clippy -p <crate> --all-targets --locked -- -D warnings`,
  `cargo test -p <crate> --locked`.
- Frontend gates: `pnpm run check`, `pnpm run lint`,
  `pnpm exec prettier --check <changed files>`, `pnpm test`.
- UI check: `pnpm dev --port <your port> --strictPort --host 127.0.0.1` in the
  background, then
  `MANANG_ORIGIN=http://127.0.0.1:<port> MANANG_CDP_PORT=<cdp> node scripts/verify-ui.mjs`;
  every check must pass. Stop your server by port afterwards
  (`lsof -ti:<port> -sTCP:LISTEN | xargs -r kill`). Never touch ports 1420
  (the owner's app) or 1431-1441 (other agents). Never `pkill -f`.
- Design (Fundaments.work, enforced by a test in `src/lib/theme`): dark
  `#141416`, acid lime `#c6f135` as the single signal, radius 0, 1px
  hairlines, DM Sans 300/400 and DM Mono 400, no shadows, no new hex codes;
  reuse `mng-btn`, `mng-icon-btn`, `text-text-60`, `border-line`. No clutter.
- Everything is a plugin: kernel commands through the typed wrappers in
  `src/lib/kernel/api.ts`; the browser dev kernel (`src/lib/kernel/dev.ts` and
  `mock*.ts`) must implement any command the UI calls. Never swallow errors:
  `reportError(where, error)` from `$lib/kernel`.
- Do NOT edit: `TopBar.svelte`, `PdfViewer.svelte`, `PreviewPane.svelte`,
  `FigureEditor.svelte` and other figure files, `src/lib/components/settings/*`,
  `src/lib/commands/*` (except where a task says so), `crates/manang-figure`,
  `crates/manang-typst`, `crates/manang-settings`, `src-tauri`.

---

## Task 1: Trash in the Home sidebar (branch `antigravity/home-trash`, port 1442, CDP 9352)

Goal: deleting a project from Home is recoverable, like any desktop app.

Today: Home (`src/lib/components/Home.svelte`, `src/lib/workbench/home.ts`
with `SECTIONS`: Projects, Recent, Pinned, Archive, Templates;
`src/lib/state/home.svelte.ts`; `src/lib/components/home/ProjectCard.svelte`)
lists the kernel's project registry (`crates/manang-project/src/registry.rs`,
commands `project.list`, `project.update` for rename/pin/archive/tags,
`project.forget`). Files deleted inside a project already go to that project's
`.manang/trash/` (`crates/manang-project/src/fsops.rs`).

Build:

1. Registry: a `trashed_at` timestamp on registry entries (absent = not in
   trash; reading old `projects.toml` must keep working, tested). Commands in
   `crates/manang-project` (declared in its `manifest.toml`, with kernel tests):
   `project.trash {path}` (marks it; files are not touched),
   `project.restore {path}`, `project.purge {path, delete_files}` (removes it
   from the registry; with `delete_files: true` moves the folder to the
   operating system's trash; if you add the `trash` crate check its licence
   and that it links no bundled C library, `cargo tree -p manang | grep -iE "harfbuzz|freetype-sys|icu4c"`
   must stay empty; never delete permanently with `remove_dir_all`), and
   `project.empty-trash {older_than_days?}`.
2. Home: a **Trash** section at the bottom of the sidebar with a count. Trashed
   projects leave every other section. In Trash, each card shows when it was
   trashed and offers Restore and "Delete…" (a confirm dialog with two
   choices: "Remove from Manang" keeps the folder on disk, "Move folder to
   system trash"), plus an "Empty trash" button with a confirm. The project
   card's action menu gains "Move to trash" (replacing any existing "Remove
   from list" that forgets immediately; keep forget reachable only from Trash).
   An open project moved to trash closes first (ask if unsaved).
3. A small setting is NOT needed now; do not edit settings files.

Tests: Rust unit and kernel tests (mark, restore, purge with and without files
on a temp dir, old registry files without the field); vitest for the section
filtering and counts in `home.ts`; verify-ui: a new section "Home trash"
after the existing Home checks: move a project to trash, it leaves Projects
and appears in Trash with a count, restore brings it back, delete with
"Remove from Manang" removes it, the folder still exists in the mock.

## Task 2: Real templates for the Templates section (branch `antigravity/templates`, port 1443, CDP 9353)

Goal: "New project" and Home's Templates section offer professional starting
points that compile.

Today: Home has a Templates section and a New project dialog
(`src/lib/components/home/NewProject.svelte`); find how templates are listed
and created (search for `template` in `src/lib` and `crates/manang-project`).

Build:

1. Templates as plain folders in the repo (for example `templates/<id>/` with a
   `template.toml`: id, name, engine, description, tags, main file), shipped
   with the app (read how the project plugin could load them; if it needs a
   Rust change, keep it inside `crates/manang-project` with tests): Blank
   Typst, Blank LaTeX, Blank Markdown, Article (Typst), Article (LaTeX),
   Thesis chapter (Typst), Letter (Typst), Slides (Typst, using the existing
   slides support), Poster (Typst), Lab report (LaTeX), Notes (Markdown with a
   Mermaid diagram fence). Each uses only packages available offline (no
   Typst Universe downloads; LaTeX: standard packages in TeX Live), has a
   `references.bib` where citations make sense, and compiles.
2. The Templates section shows them as cards with name, engine, a short
   description and a preview thumbnail if one can be generated offline from
   the first page (otherwise a clean placeholder in the design), and "Use
   template" opens the New project dialog with it selected.
3. A test that every Typst template compiles through the real Typst plugin
   (a kernel test in `crates/manang-project` or a new test file that drives
   `typst.compile` on each template folder) and that every template's
   `template.toml` is valid.

Tests: the compile test above; vitest for template listing and filtering;
verify-ui: a new section "Templates" after the Home checks: the section lists
the templates, choosing one opens New project with it selected, creating it
opens its main file.

## Task 3: User guide (branch `antigravity/user-docs`, no ports needed)

Goal: a clear, short user guide for people who are not developers.

Write Markdown files under `docs/user/` (plain English, short sentences,
active voice, no marketing tone; each page starts with what the reader can
do, then steps; keyboard shortcuts written as Ctrl+K with the macOS form in
brackets where it differs). Pages:

1. `index.md` (what Manang is, where to start, links to the pages),
2. `getting-started.md` (install from source for now, first project, Home),
3. `writing.md` (editor, Typst / LaTeX / Markdown, spell check, personal
   dictionary, formatting, snippets, symbols, word count),
4. `preview-and-pdf.md` (preview layouts, Ctrl+click to source, Show in
   preview, Export PDF, LaTeX needs a TeX installation and how to install it),
5. `figures-and-diagrams.md` (figure editor, Mermaid/Pikchr/ASCII diagrams,
   including them in documents),
6. `references.md` (bibliographies, citation styles, verify),
7. `projects-and-files.md` (Files view, opening any file type, trash, git
   view),
8. `settings-and-shortcuts.md` (Settings, user vs project scope, keyboard
   shortcuts and how to change them),
9. `plugins.md` (what plugins are, built-in vs sandboxed, what access means),
10. `troubleshooting.md` (logs location
    `~/.local/share/work.fundaments.manang/logs/Manang.log` on Linux, how to
    open devtools in a dev build with F12, common problems: no TeX found,
    blank or stuttering window on Wayland with NVIDIA and the
    `WEBKIT_DISABLE_DMABUF_RENDERER` note, a plugin that fails to load).

Verify every behaviour you describe against the code (`src/lib/commands`,
`src/lib/workbench/shortcuts.ts`, the plugins' `manifest.toml` files); where
a feature is still being built (menu bar, plugin manager, new PDF viewer
tools), say "coming soon" in one line instead of describing it. Link the
guide from `README.md`. No code changes in this task.
