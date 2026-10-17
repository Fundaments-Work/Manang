# ADR 0004: Compile LaTeX with embedded Tectonic, run in a killable worker

Status: accepted, 2026-10-17

## Context

`manang-latex` needs a LaTeX engine that works offline after first use, produces
diagnostics and SyncTeX, can be bounded by a hard timeout, and never writes into
the user's project. Research on 2026-10-17 found:

- **Tectonic is maintained.** `tectonic` 0.17.0 was published to crates.io on
  2026-07-27 (49 versions total); the GitHub repo had commits on 2026-07-31 and
  a push on 2026-08-01. 0.17.0 fixes segfaults in `\XeTeXglyphname` and macOS
  font loading, and prefetches bundle entries concurrently.
- **It embeds as a library.** `ProcessingSessionBuilder` takes a bundle, a
  primary input, an output directory, a format cache path, `keep_logs`,
  `synctex(true)` and a fixed `build_date`. It runs TeX, BibTeX and reruns
  until output is stable.
- **It builds cleanly here** (Arch, Rust 1.96). With the system `harfbuzz`
  14.5, `fontconfig`, `icu` 78 and `openssl` 3.6 installed, `cargo build -p
  manang-latex` succeeds (about 5 to 10 minutes for a cold release build, one
  time). Other Linux distributions need the same four `-dev` packages and
  `pkg-config`.
- **Bundles are fetched lazily and cached.** The default bundle is an indexed
  tar served over HTTP with range requests; used files land in a cache
  directory, and generated format files in another. A warm cache needs no
  network. A bundle can also be a directory, `.zip` or `.ttb` for fully
  offline distribution.
- **SyncTeX is supported** (`synctex(true)` writes `<name>.synctex.gz`).
- **Gaps.** `biber` is not bundled: the driver runs an external `biber` (or
  `tectonic-biber`) if one is on `PATH`; BibTeX is built in. Only XeTeX is
  available, so `pdflatex`-only packages and LuaTeX are out. A document with
  no pages makes `xdvipdfmx` fail with "cannot open main.xdv".
- **Alternatives.** No pure-Rust TeX engine is complete enough (Typst is a
  different language and is Manang's other compiler). The machine has a system
  TeX Live 2026 (`xelatex`, `pdflatex`, `latexmk`, `bibtex`) but no `biber` and
  no `tectonic` binary; depending on a user-installed TeX Live breaks
  reproducibility and the "works after first launch" promise, but it remains a
  possible fallback behind the same plugin API.

## Decision

Embed **tectonic 0.17.0** (pinned with `=`), but execute it in a **separate
`manang-latex-worker` process** per compile, driven by the plugin.

Why a worker rather than calling the library on a thread:

1. **Hard timeout.** Tectonic's C engines cannot be cancelled. A document with
   an infinite macro loop would hold the process-global engine state forever.
   A child process can be killed (verified by test).
2. **Crash isolation.** Tectonic ships segfault fixes in every recent release;
   a crash must not take down the editor.
3. **No Tokio involvement.** The host thread only waits on the child; all
   CPU work is in another process. Callers still dispatch `invoke` on Rayon
   because it blocks for the compile duration.
4. **Network lock-down.** The worker is launched with its proxy variables
   pointing at a closed local port unless `allow_network` is set, so even
   code paths that ignore `only_cached` (Tectonic still tries to fetch the
   bundle index when it is not cached) fail immediately with a clear error.

The worker reads a small JSON request file and writes `outcome.json`, the PDF,
log and SyncTeX file into a private temp directory. Document bytes never go
through JSON; the PDF returns from `latex.export-pdf` as `Payload::Binary`.
`keep_intermediates(false)` plus `output_dir` means `.aux`, `.bbl` and friends
never touch the project.

Other choices:

- **Diagnostics come from the log**, not engine callbacks, with our own
  parser (`diagnostics.rs`) for TeX and BibTeX logs. It undoes the 79-column
  wrap, tracks the `(file ... )` stack, and normalises Tectonic's `! !` and
  `..` quirks. Note Tectonic halts on the first TeX error, so a failed build
  reports one error plus earlier warnings.
- **SyncTeX is parsed in Rust** (`synctex.rs`, gz text) rather than linking the
  C parser. Forward lookup unions the boxes of a line per page; reverse lookup
  picks the smallest containing box and then the nearest glue/kern node, which
  carries the true source file for `\input`ed text.
- **Determinism.** The build date defaults to a fixed epoch (or
  `SOURCE_DATE_EPOCH`), and the shell escape is disabled. Two compiles of the
  same input produce byte-identical PDFs (tested).
- **Multi-pass** (BibTeX, reruns) is Tectonic's own loop; we get the final
  log and the `.blg`.

## Consequences

- The app must ship `manang-latex-worker` next to its executable (a Tauri
  sidecar, or `externalBin`) or set `LatexConfig::worker` /
  `MANANG_LATEX_WORKER`.
- First compile needs the network once (`allow_network: true`), or a
  pre-seeded cache directory (`LatexConfig::cache_dir`). Offline, a package
  that was never fetched is reported as an ordinary TeX "file not found" error
  with a hint. Prefetching the whole bundle (several hundred MB) is a
  follow-up.
- Measured on this machine (release worker, one-page article): warm compile
  about 0.44 s (criterion mean, includes process start); first compile with
  the bundle cached but no format file about 2.5 s, which regenerates and
  caches `latex.fmt` (slightly over the 2.0 s cold budget, paid once per
  engine upgrade and avoidable by shipping a seeded format cache); fully cold
  with network is dominated by the bundle download (minutes on a slow link).
  Log parsing runs at about 220 MiB/s (1.5 ms for a 10k-line log).
- `latex.synctex` returns a structured `[synctex_unavailable]` error when a
  build had `synctex: false`.
- Biblatex with `biber` works only where `biber` is installed.

## Follow-ups

- A persistent worker pool would cut process start-up; only worthwhile if the
  benchmark shows start-up dominating.
- Bundle prefetch / `.ttb` seed shipped with the installer for true offline
  first run.
- Optional system-TeX-Live backend for `pdflatex`/LuaTeX/`biber` users.
- Fuzz the log parser with `cargo-fuzz` (it is already property-tested).
