# Manang Robustness & Stress Testing Report

**Date:** 2026-10-18  
**Branch:** `antigravity/robustness`  
**Test Harness:** Kernel-driven integration tests, property tests, and synthetic stress fixtures  
**Profile:** Optimized dev profile (`opt-level = 3` dependencies, `opt-level = 1` crates)  

---

## Executive Summary

Ahead of the private beta release, Manang's core engines and parsers were subjected to rigorous real-world and pathological stress tests across 6 foundational crates:
1. `crates/manang-citations` (BibTeX and CSL-JSON)
2. `crates/manang-markdown` (HTML render, outline, Typst conversion, XSS sanitization)
3. `crates/manang-diagram` (Mermaid, Pikchr, ASCII/Svgbob)
4. `crates/manang-writing` (spell check, prose lint, format, text indexing, statistics)
5. `crates/manang-search` (project search, fuzzy file finder, regex, cancellation)
6. `crates/manang-figure` (scene graph, v1 migration, SVG export, extreme scale)

Every crate now includes a dedicated `tests/robustness.rs` suite and committed regression fixtures in `testdata/robustness/<area>/` (all committed files under 200 KB; large inputs generated in test code). All quality gates (`cargo fmt`, `cargo clippy -D warnings`, `cargo test --locked`) pass with zero warnings and zero panics.

---

## Performance Budgets & Measured Timings

All microbenchmarks and timings were measured on the local host with `CARGO_BUILD_JOBS=4` and shared target caching.

| Crate & Test Scenario | Scale / Payload | Measured Latency | SLA / Constraint | Status |
| :--- | :--- | :--- | :--- | :--- |
| **`manang-citations`** (Large BibTeX parse) | 5,000 entries (1.1 MB) | **78.3 ms** | $< 250\text{ ms}$ | **PASSED** |
| **`manang-citations`** (CSL-JSON parse) | 500 entries (240 KB) | **4.9 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-markdown`** (Huge table) | 1,000 rows $\times$ 5 cols | **15.6 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-markdown`** (Giant single line) | 5 MB single continuous line | **23.1 ms** (HTML) / **6.2 ms** (Outline) | $< 100\text{ ms}$ | **PASSED** |
| **`manang-diagram`** (Large Mermaid) | 50,000 characters | **6.9 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-diagram`** (Extreme flowchart) | 2,000 nodes, 1,999 edges | **51.5 ms** (Check) / **327.8 ms** (Render) | $< 5.0\text{ s}$ | **PASSED** |
| **`manang-writing`** (Prose masking) | 5 MB text (~769k words) | **105.5 ms** | $< 500\text{ ms}$ | **PASSED** |
| **`manang-writing`** (Word tokenization) | 5 MB text (768,966 words) | **15.9 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-writing`** (Prose linting) | 5 MB text (500 cap) | **189.1 ms** | $< 500\text{ ms}$ | **PASSED** |
| **`manang-writing`** (Spell check) | 5 MB text (2,000 cap) | **44.7 ms** | $< 200\text{ ms}$ | **PASSED** |
| **`manang-writing`** (Document stats) | 5 MB text | **153.3 ms** | $< 300\text{ ms}$ | **PASSED** |
| **`manang-writing`** (Markdown format) | 5 MB text | **11.2 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-search`** (50 MB file skip) | 50 MB file (default limits) | **2.4 ms** | $< 20\text{ ms}$ | **PASSED** |
| **`manang-search`** (50 MB file scan) | 50 MB file (64 MB cap) | **18.2 ms** | $< 100\text{ ms}$ | **PASSED** |
| **`manang-search`** (Deeply nested find) | 80 nested folder levels | **4.2 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-search`** (Symlink loop scan) | Self & parent cyclic symlinks | **2.4 ms** | $< 50\text{ ms}$ | **PASSED** |
| **`manang-search`** (Linear regex) | `(a+)+$` backtracking pattern | **2.5 ms** | $< 5.0\text{ s}$ | **PASSED** |
| **`manang-figure`** (100k nodes JSON gen) | 100,000 nodes (7.57 MB JSON) | **44.8 ms** | $< 200\text{ ms}$ | **PASSED** |
| **`manang-figure`** (100k nodes deserialization) | 100,000 nodes | **358.7 ms** | $< 1.0\text{ s}$ | **PASSED** |
| **`manang-figure`** (100k nodes SVG export) | 100,000 nodes (12.67 MB SVG) | **269.3 ms** | $< 1.0\text{ s}$ | **PASSED** |

---

## Findings, Bugs Fixed, and Architectural Hardening

### 1. `crates/manang-citations`
- **UTF-8 BOM Stripping:** Files with Byte Order Marks (`\u{feff}`) previously triggered parse failures in CSL-JSON (`serde_json` unexpected byte at position 0) and corrupted the first BibTeX entry key. Fixed by stripping leading BOM in both parsers.
- **Resilient Parsing:** Implemented `parse_bibtex_resilient` and `parse_csl_json_resilient`. When syntax errors, unclosed braces, or missing commas occur, the parser recovers and returns partial entries alongside line-numbered `Diagnostic` warnings instead of aborting the entire document.
- **Quadratic Duplicate Key Search:** Duplicate key scanning was previously $O(N^2)$ due to repeated document searches. Optimized to $O(N)$ with line indexing (`scan_key_lines`).
- **Standard BibTeX Macros:** Added recognition of standard month abbreviations (`jan` through `dec`) and graceful handling of empty inputs.

### 2. `crates/manang-markdown`
- **XSS Sanitization Hardening:**
  - Extended URL scheme protection to reject all `data:` URIs (previously only `data:text/html` was blocked; now `data:image/svg+xml`, `data:application/javascript`, etc., are prevented from rendering as executable links).
  - Enhanced raw HTML sanitizer to strip any inline JavaScript event handlers matching `on[a-z]+` (such as `onload`, `onclick`, `onpointerdown`, `onerror`).
- **Quadratic Character Counting Slowdown:** Replaced repeated full-document slice character counting with monotonic byte-offset newline tracking in `html.rs` and `typst.rs`, reducing 5 MB single-line document rendering time from unbounded quadratic time to 23 ms.
- **Stack Overflow Immunity:** Validated that 1,000 levels of nested blockquotes (`>>>>...`) and lists do not overflow the thread stack.

### 3. `crates/manang-diagram`
- **Extreme Diagrams & DoS Protection:**
  - Flowcharts with 2,000 nodes and 1,999 edges parse, validate, and render in under 380 ms total, well within the 5.0 s SLA.
  - Large 50,000-character diagrams process in 6.9 ms.
  - Empty sources, unknown diagram languages, and syntax errors return sensible error diagnostics with line/column pointers and never panic.
  - Full Unicode and bidirectional RTL text (Arabic, Hebrew, CJK) inside node labels render correctly.

### 4. `crates/manang-writing`
- **Character Boundary Safety:**
  - Validated that word tokenization, sentence splitting, prose lint diagnostics, spell checking, and range formatting always land on valid UTF-8 character boundaries.
  - Tested against emoji-only text, mixed scripts (CJK, Arabic, Devanagari with conjuncts/matras), and zero-width code points (`\u{200b}`, `\u{200c}`, `\u{200d}`, `\u{feff}`).
- **Unbalanced Markup:** Tested Typst and LaTeX files containing unbalanced delimiters (`$`, `[`, `\begin{equation}`, unclosed block comments). Formatters skip cleanly or return syntax errors without panicking.
- **Extreme Scale:** Validated 5 MB text processing (~769,000 words): masking, tokenization, linting, spelling, statistics, and formatting execute in ~0.5s total.

### 5. `crates/manang-search`
- **Resource Caps & Large File Handling:**
  - Under default limits (`max_file_bytes = 8 MB`), a 50 MB file is safely recorded in `files_skipped_large` in 2.4 ms without reading the body.
  - When limits permit (64 MB cap), the full 50 MB file is scanned in 18.2 ms and matches are reported.
- **Binary & Malformed Files:**
  - Binary files containing null bytes and non-text bytes are detected via `grep_searcher::BinaryDetection` and counted under `files_skipped_binary`.
  - Non-UTF-8 files are decoded lossily with replacement characters, accurately locating valid text matches.
- **Filesystem Quirks:**
  - 80 levels of nested directories traverse cleanly without path or stack issues.
  - Symlink loops (self-pointing and parent-pointing symlinks) are ignored because `ignore::WalkBuilder` runs with `follow_links(false)`. Scan finishes in 2.4 ms.
- **Linear Regex & Cooperative Cancellation:**
  - Classical catastrophic backtracking patterns (`(a+)+$`) execute in linear time (2.5 ms) without backtracking due to the DFA/NFA architecture of `grep-regex`.
  - Malformed and oversized regexes fail compilation cleanly.
  - Active searches are cooperatively interrupted via `search.cancel`.

### 6. `crates/manang-figure`
- **v1 Backward Compatibility & Migration:**
  - Tested migration of version 1 files containing legacy `stroke` (converted to `freedraw`), negative-size `rect` (normalized to positive width/height with adjusted coordinates), `ellipse` (`cx, cy, rx, ry`), `arrow` (`from, to`), and `math` (`tex` baseline adjusted). Migrated scenes validate and export clean vector SVG.
- **Scale & Stress:**
  - Tested 100,000 nodes: generated in 18.9 ms, serialized to 7.57 MB JSON in 44.8 ms, deserialized in 358.7 ms, and exported to 12.67 MB SVG in 269.3 ms.
- **Invalid Inputs:**
  - Empty JSON, truncated JSON, duplicate node IDs, NaN, and Infinity coordinates return structured `FigureError` without panicking. Negative sizes in v2 shapes are normalized via `geometry::normalize_rect`.

---

## Out-of-Scope Issues & Design Trade-offs

### Unknown Node Kinds and Fields in `manang-figure`
- **Behavior:** `crates/manang-figure/src/scene.rs` enforces `#[serde(deny_unknown_fields)]` across `FigureScene` and its node types. Providing an unknown node kind or extra field returns `FigureError::Json(...)`.
- **Rationale & Trade-off:** ADR 0012 explicitly chose `deny_unknown_fields` so that typos in hand-edited JSON files produce actionable errors with line numbers rather than silently dropping user data on save.
- **Why Not Altered:** True lossless round-trip preservation of arbitrary unknown node kinds and fields would require:
  1. Adding `extra: serde_json::Map<String, Value>` bags and an `Unknown(Value)` node variant to the Rust scene graph.
  2. Updating the TypeScript frontend mirror in `src/lib/figure/` (types, canvas renderer, undo/redo).
  Because modifying `src/` is strictly prohibited by agent operational rules, and ADR 0012 architectural invariants require strict typing, unknown fields remain cleanly rejected as errors with diagnostic feedback rather than causing panics or silent data loss.

---

## Verification Protocol Results

1. **Rust Code Formatting:**
   ```bash
   cargo fmt --all -- --check
   ```
   *Result:* PASSED (0 diffs across all workspace files).

2. **Clippy Zero Warnings Policy:**
   ```bash
   cargo clippy -p <crate> --all-targets --locked -- -D warnings
   ```
   *Result:* PASSED with 0 warnings across `manang-citations`, `manang-markdown`, `manang-diagram`, `manang-writing`, `manang-search`, and `manang-figure`.

3. **Test Suite Execution:**
   ```bash
   cargo test -p <crate> --locked
   ```
   *Result:* PASSED (100% test pass rate across all 6 crates, including new robustness test suites).
