# Computational notebooks for Manang

Date: 2026-10-18. Status: research, no code. Facts below were checked against
primary sources on this date (GitHub API, crates.io API, vendor docs); anything
not verified is marked **unverified**.

## 1. Summary and recommendation

LibrePaper lists "Computational notebooks include Quarto and Calepin"
([librepaper.org](https://librepaper.org/)); its page says nothing about how
code runs. The surprise is **Calepin**: it is not a Markdown or Jupyter format.
It is a **Typst-native** notebook tool, written in **Rust**, MIT-licensed, by
Vincent Arel-Bundock (`github.com/vincentarelbundock/calepin`, v0.0.61 released
2026-10-01, last push 2026-10-08, 195 stars). Manang already embeds Typst
0.15.1 in `manang-typst`, so Calepin's design fits us better than Quarto's.

Recommendation:

1. **Build a Typst-first notebook (Calepin-compatible chunks), not a Quarto clone.**
   Code lives in ordinary Typst raw blocks; results go to a regenerable cache
   outside the source; our embedded Typst compiler splices them in. The format
   is the user's `.typ` file, so git, diffs and live co-editing already work.
2. **Support `.qmd` by driving the user's installed `quarto`**, as we drive
   system TeX: a cheap second path with errors in Problems, no code of ours in
   the execution loop.
3. **Execute through our own small subprocess runner first** (Python, later R),
   a Jupyter ZeroMQ client second, Pyodide last.
4. **Workspace trust is the gate for everything.** Running code is arbitrary
   code execution, and our capability model cannot express it today.

## 2. Formats compared

| Format | Source | Outputs | Licence / status | Fit for Manang |
| :-- | :-- | :-- | :-- | :-- |
| Quarto `.qmd` | Markdown + YAML + ```` ```{python} ```` fences | rendered through Pandoc to HTML, PDF (LaTeX or Typst), docx; `_freeze/` stores executed results | quarto-cli MIT, v1.10.19 (2026-10-06), 6.1k stars | Largest ecosystem; heavy install (bundles Pandoc, Typst, Deno) |
| Jupyter `.ipynb` | JSON, outputs inside the file | in the file (base64 images) | BSD-3 (nbformat); universal | Poor git diffs; good as a *kernel protocol*, bad as a source format |
| Calepin `.typ` | plain Typst, raw blocks | `.calepin/<stem>/results.json` + `figures/`, gitignored, regenerable | MIT, Rust, v0.0.61 | Best fit; young, pre-1.0 |
| R Markdown | Markdown + knitr chunks | knitr (R) | knitr v1.52 (2026-09-06), GPL (**unverified** exact version) | R-only in practice; Quarto supersedes it |
| marimo | pure `.py`, reactive DAG | live, in a server | Apache-2.0, 0.25.1 (2026-10-01) | Great git story, but its own app, not a document |
| Pluto.jl | `.jl`, reactive | live | MIT | Julia-only; same story |
| Org-babel | Org + code blocks | results inline in the source | GPL, Emacs-bound | Skip: no ecosystem outside Emacs |
| Callisto (Typst package) | reads an `.ipynb` | renders what is already in the file; v0.3.0 adds a theme system and can export Typst raw elements to a notebook | Typst Universe | A *renderer*, not an executor; useful for importing existing notebooks |

Key judgements for scientific authoring:

* **Reproducibility.** All of them are only as reproducible as the user's
  environment. Quarto's `freeze` and Calepin's results cache decouple "what was
  computed" from "can I recompile". Neither pins package versions.
* **Git friendliness.** Outputs must live outside the source. `.ipynb` fails
  this (outputs and execution counts embedded, which also leaks paths and
  machine state). Quarto keeps outputs in `_freeze` and recommends committing
  it ([docs](https://quarto.org/docs/projects/code-execution.html)); Calepin
  gitignores its cache. Both give clean source diffs.
* **How outputs reach the PDF.** Quarto: Pandoc converts to Typst/LaTeX, and
  Quarto's docs say it "includes the Typst CLI", so no separate Typst is
  needed; it discards the intermediate `.typ` unless `keep-typ: true`. Calepin:
  runs a **two-pass** model around the real `typst` binary. Pass 1 queries
  chunk metadata (`calepin-mode=query`); the engines run; pass 2 compiles with
  `calepin-results=<results.json>` so a bundled Typst runtime splices outputs
  in. Figures are SVG, PNG, JPEG, or PDF (PDF on paged output only). Its
  `AGENTS.md`/`CLAUDE.md` describe a persistent Python/R subprocess per
  document with a sentinel-framed text protocol and a per-chunk timeout, plus
  a Jupyter bridge (a Python `jupyter_client` subprocess) for Julia, bash and
  other kernels.
* **Calepin caveat.** It is a binary crate, not a library, and pre-1.0
  (61 releases in a few months). We should be *compatible in spirit* (same
  chunk syntax, similar results model) but not depend on it. Because we
  embed Typst we can do the query pass in-process, with no `typst` binary.
* **Typst-only options today.** Callisto reads outputs but does not execute.
  I found no other maintained Typst package that runs code; Typst itself
  cannot spawn processes (WASM plugins only), which is why Calepin is an
  external preprocessor.

## 3. Execution architectures

### (a) Drive the user's `quarto`

* Needs: `quarto` on PATH (bundles Pandoc, Typst, Deno), plus Python with
  `jupyter` (the Jupyter engine) or R with `knitr`, or Julia. Detect with
  `quarto --version`; a `quarto check` command exists in Quarto but I could not
  confirm its output format from the pages I fetched: **unverified**. Parse
  stderr for the `file.qmd:line` form of errors for Problems.
* Costs: cold start of Quarto plus a kernel is seconds, not milliseconds, and
  it re-executes on every incremental render (the docs say incremental renders
  always execute code); `freeze: auto` helps only for project renders. Cache
  (`cache: true`) uses Jupyter Cache or knitr cache. Fine for "Render" and
  export, wrong for a keystroke preview loop.
* Our rule on bundled C libraries is untouched: it is an external process.

### (b) A Jupyter kernel client in Rust

* Crates (crates.io API, 2026-10-18): `jupyter-protocol` 2.0.2 (BSD-3,
  2026-06-27), `runtimelib` 3.0.0 (BSD-3, 2026-05-05; its manifest now says
  it is deprecated and re-exports `jupyter-zmq-client` 1.0.1), `nbformat`
  3.0.0, all from `runtimed/runtimed` (last push 2026-07-22). They sit on
  `zeromq` 0.6.0 (**pure Rust**, MIT, `zeromq/zmq.rs`, last push
  2026-09-08, tokio runtime optional). `jupyter-zmq-client` depends on it with
  default features off (tcp and ipc only) and `ring` (a C/asm crypto crate that
  does not replace a system library) for HMAC signing.
* **Forbidden alternative:** `zmq` 0.10.0 (last release 2022-11-04) builds on
  `zmq-sys`, which has a `zeromq-src` dependency, i.e. it can compile libzmq in
  statically. Reject it per AGENTS.md section 7.4.
* Tokio is I/O only, which matches: kernel sockets are I/O; our Rayon pool is
  not involved.
* Work: kernelspec discovery (`jupyter --paths`, `kernel.json` under data dirs,
  venv/conda/uv environments), spawn with a connection file, heartbeat,
  `execute_request` to `execute_reply`/`iopub`, interrupt (signal on unix;
  `interrupt_request` where the spec says so), restart, stdin requests, MIME
  bundles (`image/png`, `image/svg+xml`, `text/html`, `text/latex`,
  `application/json`, plotly/vega). Skip comm widgets. Effort is real:
  roughly three to four weeks to a robust client, mostly lifecycle edge cases.
* Native kernels still need Python and `ipykernel` installed by the user.

### (c) Our own runners and zero-install Python

* **Minimal subprocess runner** (what Calepin does): a tiny Python and R
  script embedded in the app, speaking a framed JSON protocol over stdio.
  Cheap (about one week for Python), persistent state per document, easy
  timeouts (kill on hang), figures captured by a matplotlib hook. Weakness: it
  needs Python, and we reinvent a slice of Jupyter.
* **Pyodide** (MPL-2.0; current release 314.0.7, 2026-09-14): the
  `pyodide-core` tarball is 6.7 MB, the full distribution 337 MB. numpy and
  matplotlib are separate wheels loaded on demand; their sizes I did not
  measure (**unverified**). It runs in a browser engine, so inside Tauri it
  means a hidden sandboxed webview or worker, with no Rust-side execution,
  limited packages (no arbitrary C extensions beyond the Pyodide set), and
  slower than native. It is, however, the only zero-install path that is also
  web-capable (AGENTS.md invariant 5) and a genuinely strong sandbox, since
  the page has no filesystem or network by default. Compare TeX: Tectonic
  bundles run from our sidecar for LaTeX; for Python there is no equivalent
  single static binary.

### (d) Typst scripting

Already deterministic and in-process; it is not a notebook (no state, no I/O,
no plotting beyond CeTZ/Lilaq). It remains the right answer for "compute in the
document" and needs no trust gate.

### Security

Running a cell executes arbitrary code with the user's rights. Findings:

* **Our capability model cannot express it.** `crates/manang-kernel/src/capability.rs`
  defines `Capability::{Filesystem, Network, Buffer}` only; there is no
  process/exec capability, and WASM plugins get no syscalls. A runner is
  therefore a **first-party native plugin** at first (like `manang-latex`'s
  sidecar), and a third-party runner needs a new `process` capability with a
  named, allow-listed executable, never a free command string.
* **Workspace trust** (VS Code model): an untrusted folder opens in
  restricted mode (read, edit, Typst-only rendering); the first Run asks
  "Trust this project to run code?" per project, stored outside the project
  (app data dir, keyed by canonical path, never by a file in the repo, or a
  cloned repo could trust itself). Re-prompt if the runner or interpreter path
  changes. Document this as consent, not a sandbox.
* **Sandboxing** is feasible but uneven, all **unverified here**: bubblewrap or
  Landlock on Linux (read-only root, writable project and cache, network off),
  `sandbox-exec` on macOS (deprecated, still shipped), Job Objects plus
  restricted tokens on Windows (limits and kill-on-close, not filesystem
  isolation). Be honest in the UI: "limits memory and time; does not stop code
  from reading your files" unless the Linux sandbox is on.
* **Limits:** per-cell timeout (default 60 s), output size cap, memory cap
  where the OS offers one, kill the process group on Stop, network off by
  default when sandboxed, visible "network allowed" indicator otherwise.
* Never execute on open, on preview, or from co-editing input. A remote peer's
  edit to a code cell must not run on my machine (see section 4).

## 4. UX in our language

Fixed vocabulary: no new top-level noun. Notebooks add no sidebar entry.

* **Cells are decorated code blocks in the editor.** A fenced block with an
  execution language gets a one-line hairline header: language, counter
  `[3]` or `[ ]`, and Run / Stop. Only the focused cell's Run uses the lime
  accent (one action per view). Run all and Restart live in the command
  palette and the editor toolbar overflow. Keyboard: Shift+Enter runs and
  advances.
* **Outputs render below the cell** in the editor as a CodeMirror block widget
  (text in DM Mono, images and SVG as tonal blocks, errors in the existing
  diagnostic style) and in the preview through the normal Typst compile. Tables
  come back as Typst/Markdown or HTML-to-text; figures are SVG first (our
  pipeline is vector-deterministic), PNG fallback.
* **Storage:** `.manang/outputs/<doc-stem>/results.json` plus `figures/`, keyed
  by a hash of (cell source, options, language, interpreter id, upstream cell
  hashes for stateful languages). Gitignored by default (Calepin does the same);
  a "commit outputs" toggle is the equivalent of Quarto's `_freeze`, so
  collaborators without Python can still build the PDF. The `.typ` or `.qmd`
  stays clean and diffs stay readable.
* **Stale outputs:** a cell whose hash differs from its stored result shows a
  muted "stale" marker; downstream cells in a stateful session are marked
  stale after an edit upstream (conservative, no reactive DAG in v1).
* **Preview and export:** the preview never executes code on keystrokes; it
  shows cached results (stale ones marked). Explicit "Run all, then build"
  executes, then compiles. Export to PDF uses the same results file, so Typst
  and LaTeX exports stay deterministic from a frozen cache.
* **Status bar and panels:** one status item (kernel: `python 3.12 idle`,
  click to pick an interpreter or restart); logs and stderr in **Output**;
  runtime errors and chunk failures in **Problems** mapped to the cell line;
  variables inspector deferred (it would need a new panel, which breaks the
  vocabulary rule; if wanted, a sub-view of Output).
* **Live co-editing.** Source text (including code) is shared. Execution is
  local to each participant, and results are **not** shared by default: each
  person's cache is their own. A peer's cell edit marks the cell stale for
  me and never runs it. Decision needed: whether the host may publish their
  results as a read-only snapshot to peers (convenient, but leaks data and
  makes non-reproducible values look authoritative). I recommend no for v1.

## 5. Which documents first

1. **Typst with chunks (first).** Calepin-style: raw blocks with a language
   tag, options in a comment header or a `#calepin`-like setup call, results
   spliced by our compiler. Honours the "documents stay readable UTF-8 files"
   invariant and uses the engine we own. Compatibility target: documents that
   work with Calepin's chunk syntax should open and mostly work.
2. **Markdown and `.qmd` through installed Quarto** (second): render, preview of
   the output, errors in Problems. Inline run-cell for ```` ```{python} ```` can
   reuse the same runner by treating the Quarto fence as a chunk, but do not
   try to reimplement Quarto's cross-references, callouts or shortcodes.
3. **LaTeX** (last): knitr/Sweave-style is niche and R-bound; point users at
   Quarto, or at a `.qmd` that targets LaTeX.

What we should **not** build: a Pandoc/Quarto clone; reactive dependency
graphs; comm widgets and interactive HTML widgets in the PDF path; a `.ipynb`
editor (offer import to a source format via the Callisto route or `nbformat`);
a hosted kernel service; a package manager for Python.

## 6. Plugin architecture

Notebook support is a **document-type extension plus a runner**, on the
contribution points in `docs/plans/2026-10-18-platform-v2.md` (section 3.2).
That plan lists document-types, importers/exporters, views, commands,
settings, keybindings and themes, and no runner.

Proposed new contribution point:

```toml
[[contributes.runners]]
id = "python"
languages = ["python", "py"]
executable = { detect = ["python3", "python"], min-version = "3.9" }
protocol = "stdio-json-v1"        # or "jupyter" with kernelspec = "python3"
interrupt = "signal"
```

and a manifest permission `process = ["python3"]` (named executables only)
next to `filesystem`, `network`, `buffer`. The plugin manager's consent dialog
should render it in plain language: "Can run `python3` programs on your
computer, with the same access as you. Cannot be limited by Manang unless
sandboxing is enabled." Filesystem and network lines apply to the sandbox
profile, not the interpreter. Third parties could then add Julia, R or Lua
runners; for WASM-tier plugins the runner itself stays native-host-owned (the
plugin only declares and parses; the host spawns), which keeps invariant 4.

Pairing: `manang-notebook` (document-type extension: chunk parsing, results
store, trust, decorations, preview integration) plus `manang-runner-python`
(first-party runner). Recommended split of crates and files, disjoint:
`crates/manang-notebook/` (parsing, store, trust, commands),
`crates/manang-runner/` (process and protocol, timeouts), editor UI in
`src/lib/notebook/`.

## 7. Roadmap

### Horizon A: beta slice (about 3 weeks)

Cheapest useful thing, both paths:

* **WP1 Trust (2 d)**: workspace trust store, prompt, `notebook.trust`,
  `notebook.status`; tests that an untrusted project never spawns a process.
* **WP2 Chunk model (3 d)**: parse Typst raw-block chunks and Quarto fences
  into `Chunk { id, lang, options, span, hash }`; insta snapshots; proptest on
  span stability under edits.
* **WP3 Python runner (4 d)**: embedded script, stdio JSON protocol,
  `notebook.run-cell(chunk)`, `notebook.run-all`, `notebook.stop`,
  `notebook.restart`; timeout, output cap, matplotlib figures to SVG; Rayon
  not needed (process I/O on Tokio).
* **WP4 Results store and splice (4 d)**: `.manang/outputs` format, stale
  hashing, a Typst runtime shim that renders outputs through `manang-typst`
  via `sys.inputs`; errors to Problems.
* **WP5 Editor UI (3 d)**: cell header decorations, output widgets, status
  item, Output panel logs.
* **WP6 Quarto driver (2 d)**: detect `quarto` and Python, `quarto.render`
  to PDF/HTML, stderr to Problems, preview of the result. Tests skip when
  tools are absent (and fail under a `REQUIRE_TOOLS` env var, as Calepin does).

Risks: Python discovery on Windows and macOS, stateful hashing.

### Horizon B: full Jupyter client (4 to 6 weeks)

`crates/manang-jupyter` on `jupyter-protocol` + `zeromq`: kernelspec discovery,
lifecycle, MIME rendering (HTML sanitised in a sandboxed frame), stdin,
`process` capability and runner contribution point in manifest v2, third-party
runner plugins, Linux bubblewrap profile, ipynb import/export.

### Horizon C: zero-install and reactive (open-ended)

Pyodide in a hidden sandboxed webview with numpy/matplotlib, offline bundle
(size to be measured; the core alone is 6.7 MB), `.manang` reactive mode
(cell dependency graph, marimo style) only if users ask.

## 8. Decisions the owner must make

1. **Typst-first (Calepin-compatible) or Quarto-first?** I recommend Typst-first
   with Quarto as an external driver.
2. **Chunk syntax compatibility** with Calepin: copy it exactly, or our own?
   (Exact copy buys users and docs; costs us a moving target at v0.0.x.)
3. **Outputs committed by default?** I recommend no, with an opt-in.
4. **Trust model:** per-project prompt as above, and whether any code runs
   without it (I say never).
5. **Share results among live collaborators?** I recommend no for v1.
6. **Is a first-party-only runner acceptable for beta,** deferring the
   `process` capability and third-party runners to Horizon B?
7. **Zero-install priority:** is "works without Python" a beta requirement
   (then Pyodide moves up and the slice grows by about three weeks)?
