# Self-healing compile errors (a plugin)

## Goal

When a document fails to compile because of something small and mechanical (a forgotten `\usepackage`, a missing environment package, a Typst package not imported, a missing closing delimiter), Manang offers a one-click fix, shows exactly what it changes, applies it as one undoable edit, and recompiles. Larger problems go to "Fix with AI" (already in the agent), as the second tier.

## What exists

* LaTeX diagnostics name the undefined command and carry a `hint` ("add `\usepackage{graphicx}`") from `package_for` and `package_for_environment` in `manang-latex/src/diagnostics.rs`.
* Typst diagnostics carry `hints` from the compiler.
* The editor can show diagnostics with a one-step `QuickFix` (`src/lib/editor/diagnostics.ts`, used by the writing checks).
* The agent has "Fix with AI" on Problems rows.

## Design

A first-party plugin crate `crates/manang-heal` (pure Rust, wasm32-capable, no OS access), registered like every plugin, command ids `heal.*`:

* `heal.suggest { engine, file, text, diagnostics[] }` returns `fixes[]`. Each fix: `id`, `label` ("Add \usepackage{graphicx}"), `why` (one sentence), `edits[]` (`from`, `to`, `insert` as UTF-16 offsets, the unit the editor uses), `confidence` (`certain | likely`), `rule`.
* Rules are small pure functions over the diagnostic and the text, each with tests and a snapshot (`insta`):
  * LaTeX: undefined control sequence to its package (reuse and move the tables into this crate; `manang-latex` depends on it for the hint text); unknown environment to its package; `\usepackage` placement (after the last `\usepackage`, else after `\documentclass`, never inside the preamble of an included file); do not add a package that is already loaded; `tikz` libraries (`\usetikzlibrary`); missing `\end{...}` / unbalanced braces when the cause is unambiguous; `\includegraphics` without `graphicx`; bibliography commands without `\bibliography`/`biblatex`.
  * Typst: unknown variable or function that matches a known `@preview` package's exports (a curated, versioned table: cetz, fletcher, touying, tablex, ...) to `#import "@preview/<pkg>:<ver>": *`; unclosed delimiters; `#set`/`#show` misspellings with a single close match (edit distance 1).
  * Markdown: unclosed code fences, broken table pipes.
* Every fix is deterministic and explainable. No network. No guessing between candidates: if two packages provide a command, offer both as separate fixes and say so.

### Closed loop (verification)

A fix is marked `verified` when applying it to a scratch copy and recompiling reduces the error count and introduces none. v1 verifies in the app: apply, recompile, and if the error count did not drop, offer "Undo this fix" in the notice. v2 verifies before showing it (scratch compile through the existing compile commands with an in-memory override), so only fixes that worked are offered as certain.

### Surface

* Problems rows and the editor's diagnostic tooltip get a **Fix** action (the CodeMirror lint `actions` mechanism, the same as the writing checks). One fix applies directly; several open a small list.
* After a fix: a status notice "Added \usepackage{graphicx}. Undo" and the usual recompile.
* **Fix all** when several diagnostics have certain fixes (applied as one transaction, one undo).
* Setting `heal.auto-apply` (off by default): apply certain fixes on save. Policy key to disable (`heal.enabled`).
* The agent receives the unresolved ones: "Fix with AI" stays on rows with no deterministic fix.

## Work packages

1. `manang-heal` crate: rule engine, LaTeX and Typst rules, tests, snapshots, a small corpus of real broken documents compiled in tests (system TeX tests are gated like the existing ones).
2. Plugin wiring: manifest, `heal.suggest`, kernel-driven tests, mock for the browser kernel.
3. UI: lint actions, Problems row Fix, notices, Fix all, Settings page, docs, verify-ui section.
4. Verification loop v2 and the Markdown rules.

## Out of scope

Rewriting prose, fixing logic errors, anything needing the network or the agent (that is "Fix with AI").
