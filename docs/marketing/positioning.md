# Positioning, search and launch

How we describe Manang, to whom, and where people should find it. Every claim here must be true of the build that is shipping; when a claim is about something in development, say so.

## One sentence

Write papers, theses and slides in Typst, LaTeX or Markdown, and draw the figures, in one fast desktop app that keeps your files on your disk.

## Who it is for

1. **Researchers and PhD students** writing papers and theses who want Overleaf's convenience without a browser, a login or a queue, and who need good figures.
2. **Typst adopters** who want an editor with a real preview, figures and references built in.
3. **Teams and labs** who need privacy: local files, Git, end-to-end encrypted live editing, a global AI off switch, sandboxed plugins.
4. **Teachers** making lecture notes and slides from one source.

## Pillars (use in this order)

1. **One app, three formats, real previews.** Typst, LaTeX, Markdown; a PDF viewer you can select, search and click through.
2. **Figures inside the document.** Figure studio, formulas, Mermaid, connectors that stay attached; `.figure` source plus SVG.
3. **Your files, your disk.** Plain UTF-8, Git-friendly, no account, no telemetry, never lose work.
4. **AI that proposes, you decide.** Bring your own key; see what is sent; accept or reject diffs.
5. **Write together, privately.** Share by Git; live co-editing encrypted end to end (in development for the beta).
6. **A plugin platform.** Everything is a plugin; third-party plugins are sandboxed WebAssembly with declared permissions.

## What we do not claim

No "Overleaf killer", no benchmark numbers until we have measured them in the real app, no "zero-install LaTeX" until it ships, no public availability while the beta is invite-only. Competitors are named only for facts anyone can verify (for example, Overleaf is a hosted web service).

## Search and discovery

**GitHub description (under 350 characters):**
Local-first scientific writing: Typst, LaTeX and Markdown with live PDF preview, a built-in vector figure studio, AI that proposes diffs, and encrypted live co-editing. Rust, Tauri, Svelte. Private beta.

**Topics:** typst, latex, markdown, scientific-writing, academic-writing, overleaf-alternative, local-first, offline-first, desktop-app, tauri, rust, svelte, vector-graphics, diagrams, mermaid, collaborative-editing, crdt, ai-writing-assistant, pdf, research-tools.

**Keywords to use naturally in the README, docs titles and release notes:** Typst editor, LaTeX editor, offline LaTeX editor, Overleaf alternative, local-first, scientific writing, thesis writing, academic writing, figure editor for papers, Mermaid in LaTeX, SyncTeX, bring your own key.

**Social preview:** `docs/screenshots/social-preview.png` (1280x640). Upload in Settings > General > Social preview (GitHub offers no API for it).

**Pages to publish at launch (on fundaments.work, not in this repo):** a landing page with the pillars above and the screenshots; a "Manang vs Overleaf" page limited to verifiable facts; a changelog page fed from `CHANGELOG.md`; a short post on the plugin platform and one on the encrypted live editing design (ADR 0013).

## Beta invitation copy (short)

> You are invited to the Manang private beta: a desktop app for writing papers in Typst, LaTeX and Markdown with built-in figures, AI that only proposes, and live co-editing. Your link is personal. Please tell us what breaks: Help > Send feedback builds a report you read before you send it.

## Checklist before the repository goes public

- [x] LICENSE (MIT, holder Fundaments.work: confirm), CODE_OF_CONDUCT, SECURITY, SUPPORT, CONTRIBUTING, issue and PR templates, Dependabot, CodeQL, dependency review.
- [x] Secrets scan of history (2026-10-18, pattern scan): only obvious fakes in redaction tests and AWS's documented example key. Re-run with gitleaks on the final history.
- [ ] Decide what history goes public. Today's history has (1) about 200 MB of build output from an early commit (`echo-guest/target`, removed later but still in the pack, 225 MB `.git`) and (2) a personal email address as the author of all 399 commits. Options: publish a squashed or filtered history to the public repository (recommended: `git filter-repo` with `--path ... --invert-paths` and a `.mailmap` to a noreply address, done on a fresh clone) and keep this private repository as the archive; or open the full history as is.
- [ ] Replace sample-data screenshots with desktop screenshots.
- [ ] Set description, topics and social preview; enable Discussions and issue templates.
- [ ] Tag `v0.1.0-beta.1` with release notes from `docs/beta/release-notes-template.md`.
