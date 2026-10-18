# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Send feedback and Report a problem:** Help > Send feedback opens a dialog with a message field and a diagnostics report (versions, plugins, the end of the log, home folders and key-like strings removed) that the person reads and copies; nothing is sent by Manang. About now shows the real version, commit, channel and system, and what is new from this changelog.
- **Self-healing fixes:** the new `manang-heal` plugin offers deterministic one-click fixes for small compile errors (a missing `\usepackage`, TikZ library, `\end{document}`, unbalanced braces; Typst `@preview` imports, keyword slips and unclosed delimiters; Markdown unclosed fences and table rows). A **Fix** button on Problems rows, an editor quick fix (`Ctrl+.`), **Fix all** as one undoable edit, an Undo notice that also reports a fix that did not help, and `heal.enabled` / `heal.auto-apply` settings. See `docs/user/fixing-errors.md`.
- **Vector Figure Studio:** In-house typed SVG scene graph (v2) supporting freehand ink (`perfect-freehand`), geometric shapes (Rect, Ellipse, Diamond, Line), bound arrows with auto-snapping to shape anchors, container frames, selection scaling, and deterministic headless SVG export.
- **Mathematical Formulas in Figures:** Dedicated Typst and LaTeX formula nodes in the Figure Studio with docked properties and custom color palette.
- **Text Diagram Support:** Integrated compilers for Mermaid (`.mmd`), Pikchr, and ASCII art (`svgbob`) generating standalone SVGs; automatic diagram rendering in Markdown code fences.
- **Multi-Document Text Editor:** CodeMirror 6 editor powered by `manang-buffer` (`ropey::Rope`) with coordinate mapping, line indexing, and bounded transaction undo/redo history.
- **Multi-Engine Compilation & Live Preview:** Sub-50ms incremental compilation and live streaming SVG preview for Typst, LaTeX (via system TeX or optional worker), and Markdown.
- **Bidirectional SyncTeX Navigation:** Double-click or `Ctrl`-click (`Cmd`-click on macOS) in the preview jumps directly to the matching source line; `Ctrl+Alt+J` (`Cmd+Alt+J`) jumps forward from editor to preview.
- **PDF Preview Viewer:** Native PDF viewer with text layer selection and copy (`Ctrl+C`), find bar (`Ctrl+F`) with match counter, zoom presets and continuous wheel zoom, page box navigation, dark pages mode, and outline navigation.
- **Version History & Snapshots:** "Save a version" (`Ctrl+Alt+S`), commit history timeline, split and unified diffs, author identity setup, and dirty/clean file restoration.
- **Recoverable Home Trash:** Safe project deletion via `project.trash`, `project.restore`, `project.purge` (remove from list or move to OS trash), and `project.empty-trash` alongside a dedicated Trash section in the Home sidebar.
- **Starter Templates Library:** 11 built-in templates across Typst, LaTeX, and Markdown with `template.toml` manifests and offline compilation verification.
- **Welcome Onboarding Template:** Interactive onboarding starter project (`templates/welcome/`) featuring `main.typ`, modular chapter includes, bibliography, hand-crafted SVG illustration, Mermaid flowchart, and companion Markdown notes.
- **Plugin Manager & Plugins View:** Plugin lifecycle management (`Ctrl+Shift+X`) supporting installation from local directory or `.manangplugin` zip archive, capability consent auditing, enable/disable toggling, and uninstallation to trash.
- **Search & Replace Panel:** Multi-mode search featuring fuzzy file name search (`search.files`), regex and literal text content search with glob filters, line highlighting, and atomic replace preview/apply.
- **Source Control Panel:** Git integration supporting staged/unstaged file groupings, unified/split diffs, commit with amend, branch switching, and remote repository sync.
- **References & Citations Panel:** Automated indexing of BibTeX and CSL-JSON bibliographies, ranked search, APA and numeric/IEEE citation formatting, cursor insertion, and online verification against Crossref, OpenAlex, and arXiv.
- **Application Menu Bar:** Standard desktop menu bar (File, Edit, View, Go, Build, Help) with `Alt+letter` mnemonics, `F10` keyboard focusing, responsive narrow-window folding, and customizable keybindings in Settings.
- **AI Agent Foundations:** `manang-agent-core` orchestration types, policy gates, OS keychain credential storage (`manang-keystore`), Anthropic and OpenAI-compatible streaming clients (DeepSeek preset), and Rig adapter evaluation.
- **Realistic Offline Test Corpus:** 12 offline test projects under `testdata/corpus/` covering Typst, LaTeX, and Markdown with an automated compiler test harness enforcing < 5.0 s compile SLA.
- **User Documentation:** 10-page non-developer user guide under `docs/user/` and complete menus reference.
- **Beta Documentation & Issue Forms:** Tester guide (`docs/beta/tester-guide.md`), living known issues list (`docs/beta/known-issues.md`), release notes template (`docs/beta/release-notes-template.md`), and GitHub issue forms (`bug_report.yml`, `feature_request.yml`, `beta_feedback.yml`).
- **Contributing Guide & Architecture Map:** Comprehensive `CONTRIBUTING.md` with build instructions, port conventions, quality gates, and an exhaustive map of all crates and frontend directories.

### Changed
- **Minimum Supported Rust Version (MSRV):** Elevated workspace MSRV to Rust 1.95 (`edition = "2024"`).
- **Cargo Profiles:** Optimized dependency compilation in dev profile (`opt-level = 3` for external crates, `opt-level = 1` for local crates) to maintain realistic preview performance.
- **Light Theme Contrast Calibration:** Calibrated light theme accent token to `color-mix(in srgb, #c6f135 35%, #1c1c1c)` (`#586725`), achieving $\ge 5.02:1$ contrast ratio meeting WCAG 2.1 AA requirements.
- **Project Deletion Behavior:** Converted immediate irreversible project removal into a recoverable two-step trash workflow.

### Fixed
- **Linux Wayland + NVIDIA Stutter:** Automatically set `WEBKIT_DISABLE_DMABUF_RENDERER=1` at startup to eliminate WebKitGTK DMABUF rendering glitches under NVIDIA Wayland sessions.
- **SVG Float Jitter:** Replaced floating-point output drift with stable 1-decimal coordinate formatting (`format_coord`) in headless figure export.
- **Project Scaffolding Escaping:** Added escaping for special characters in project titles to prevent markup parse failures during scaffolding in Typst and LaTeX.
