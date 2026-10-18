# Release day: core things for Beta 1

Goal for today: a build the owner can install and hand to the first invited testers, with everything a tester touches on day one working. Scope is what `phase-2.md` calls Beta 1 minus what needs accounts we do not have yet.

## Where we are (main)

Done: editors, Typst/LaTeX/Markdown preview, figures and diagrams, plugin platform, data safety (recovery, formats, safety copies), AI agent v1, version history and restore, Trash, templates, robustness suites, accessibility fixes.

Not done: Share and Get updates; merge and conflict resolver; live co-editing (crates exist, not wired); feedback and diagnostics bundle; first-run onboarding and in-app help; installers, signing, update channel; CI on main (branch `ci/initial`, UI job failing); self-healing of compile errors.

## Tracks

Concurrency budget (RAM): at most two build-heavy agents, one Antigravity session, UI runs take turns.

| Track | Who | Work | Done when |
| :--- | :--- | :--- | :--- |
| A Sharing | Claude agent | `manang-git` remote: connect a remote, Share (push), Get updates (fetch and merge, fast-forward first), status; Share/Get UI in the Versions view; conflict detection that stops with a clear message (the full resolver follows) | Two clones exchange changes through a bare remote in kernel tests; UI states covered in verify-ui |
| B Live editing | Claude agent | Wire `manang-live` and `manang-relay` as the `live` plugin: start/join/leave a session on the open file, encrypted invite link, remote cursors in CodeMirror, session bar; file on disk stays plain UTF-8 | Two kernel clients converge through a local relay in tests; UI session flow in verify-ui |
| C Feedback and release plumbing | Lead | merge CI and fix the UI job; diagnostics bundle (read before send) and Send feedback; version and channel in About; What's new; Files rename, hidden files, figure naming; bundle config and a Linux release build | CI green on main; a Linux AppImage/deb builds from a clean checkout |
| D Light work | Antigravity | first-run sample project, in-app Help pages from `docs/user`, empty and error states audit, copy pass | pnpm check, lint, vitest, verify-ui green |
| E Self-healing | Lead, after A or B merges | deterministic quick fixes for LaTeX and Typst errors (missing package), applied as proposals, with the compile re-run to verify; Fix with AI as the second tier | A forgotten `\usepackage` is fixed in one click |

## Not today

Windows and macOS signing (needs the Apple account and a Windows certificate), notarisation, relay hosting (decide: Cloudflare Worker), notebooks, visual editor, WASM TeX, `.docx`, policy layer. Installers for Windows and macOS build in CI later; today's artifact is Linux.

## Merge order and gates

One branch per track, merged `--no-ff` with the section 5 gates after each; docs-only branches first; B and A both touch the kernel command lists, so expect test-count conflicts in `plugins.test.ts` and `lib.rs` (keep the union).

## Owner decisions still open

Apple Developer account; Windows certificate; relay host; cursor colours; per-tester link host.
