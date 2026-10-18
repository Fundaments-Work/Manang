# AGENTS.md — Development Guidelines & Operational Rules for AI Agents

**Project:** Manang (The High-Performance Scientific Authoring & Illustration Suite)  
**Target:** AI Coding Agents (Antigravity, Cursor, Claude Code, Copilot, etc.) and Human Engineers  
**Version:** 1.2.0  
**Last Updated:** 2026-10-17

---

## 1. System Identity & Mission

Manang is an uncompromisingly fast, local-first scientific authoring and illustration suite combining **Typst**, **LaTeX (Tectonic)**, **Markdown**, an in-house **vector figure studio**, and a **CeTZ / TikZ Visual Studio**. 

Every line of code committed to this repository must preserve three foundational pillars:
1. **Sub-millisecond responsiveness:** No UI thread stalls, no unnecessary memory allocations, zero-copy IPC.
2. **Absolute reliability & memory safety:** Rust-first engineering, zero unchecked panics, isolated WASM plugins, and lossless CRDT document storage.
3. **Extensibility & reproducibility:** Clean modular boundaries, capability-based sandboxing, and deterministic compilation.

---

## 2. Absolute Architectural Invariants (Non-Negotiables)

When writing or modifying code in this codebase, you MUST adhere to the following invariants:

1. **Rust-First Core Engine:** All compute-heavy operations (text buffers, parsing, compilation, citation indexing, plugin execution, and agent orchestration) MUST live in Rust. The frontend is strictly a thin presentation and event-dispatching shell.
2. **Zero-Copy Binary IPC:** Never serialize large files, PDFs, or vector SVGs into base64 or heavy JSON across the Tauri IPC boundary. Always use typed raw binary buffers (`Uint8Array` / `tauri::ipc::Response`).
3. **Thread Safety & Isolation:**
   - **Tokio Runtime:** Reserved strictly for asynchronous I/O, network requests, agent streaming, and file watching. Never run CPU-intensive compiler passes inside Tokio worker threads.
   - **Rayon Thread Pool:** Used for CPU-bound tasks (Typst evaluation, Tectonic passes, image rendering, syntax tree re-parsing).
   - **UI Thread:** Must remain at 60/120 FPS at all times. Keystroke-to-screen latency must stay under 8ms.
4. **WASM Sandboxing for Extensions:** Third-party plugins must NEVER be granted direct OS syscall access. They must execute inside **Wasmtime component-model** sandboxes (see ADR 0003) with explicit capability declarations in `manifest.toml`. Capabilities are linked as WIT imports; there are no ambient WASI syscalls.
5. **Web-capable core:** a web version is planned after the desktop core. Plugin logic that does not inherently need the OS must compile to `wasm32-unknown-unknown`; filesystem, network, process spawning and clocks go behind small traits/services so a browser implementation can be swapped in. Do not add OS-only dependencies to such crates.
6. **Local-First & Git-Native:** Documents must remain human-readable UTF-8 files on disk. Never introduce proprietary binary database formats that trap user files.

---

## 3. The Do's and Don'ts

### Rust Engineering

| Category | DO | DO NOT |
| :--- | :--- | :--- |
| **Error Handling** | Use `thiserror` for library crates with structured error enums; use `eyre`/`anyhow` only at top-level binary entry points. | NEVER use `.unwrap()` or `.expect()` in non-test code. Handle all `Option` and `Result` types explicitly. |
| **Memory & Borrowing** | Prefer borrowing (`&T`, `&str`, `&[T]`) over cloning. Use `Cow<'_, T>` when ownership is conditional. | Do not use `.clone()` to appease the borrow checker without a profiling justification. |
| **Data Structures** | Use `ropey::Rope` for document buffers and Tree-sitter for incremental syntax parsing. | Do not manipulate multi-megabyte text documents as standard `String` or `Vec<u8>` buffers. |
| **Concurrency** | Use lock-free channels (`crossbeam-channel`) or `parking_lot` primitives where mutexes are required. | Do not hold `std::sync::Mutex` guards across async `.await` yield points. |
| **Panics** | Always return `Result<T, E>`. Use `catch_unwind` only at FFI/WASM plugin host boundaries. | NEVER write `panic!`, `todo!`, or `unimplemented!` in production modules. |

### Frontend & UI Engineering

| Category | DO | DO NOT |
| :--- | :--- | :--- |
| **State Management** | Keep state close to the leaf components using Svelte 5 runes (`$state`, `$derived`). Use immutable CodeMirror 6 transactions and Yjs docs. | Do not create monolithic global state stores that cause whole-page re-renders. |
| **Canvas Integration** | Use the in-house typed SVG scene graph (`manang-figure`) with `perfect-freehand` stroke geometry. Persist raw scene JSON alongside headless-rendered vector SVGs. | Do not pull in proprietary, source-available, or raster-only canvas SDKs (tldraw, Fabric/Konva, React-bound Excalidraw) that require commercial licenses, inject watermarks, or break deterministic vector export. |
| **Styling & Assets** | Strictly adhere to the **Fundaments.work Design System** (source of truth: https://fundaments.work/brand/): `--radius: 0` everywhere, 1px hairlines (`#3d3d42`), DM Sans (300/400), DM Mono (400), dark base (`#141416`), and accent Acid lime (`#c6f135`, one action per view) with Sage (`#b3d17a`) as secondary. | NEVER introduce rounded corners (`rounded-*`), drop shadows, arbitrary hex codes, or font weights outside 300 and 400. Depth is strictly tonal. |
| **Bundle Size** | Keep frontend dependencies minimal. Leverage Vite code-splitting and dynamic imports for heavy viewers. | Do not import large JS libraries (e.g. Monaco, Moment.js, heavy MathJax bundles) when lighter alternatives exist. |

---

## 4. Rigorous Testing Harness & Quality Gates

Every non-trivial feature or refactor must be accompanied by comprehensive tests across multiple testing layers:

```
┌────────────────────────────────────────────────────────┐
│                   TESTING PYRAMID                      │
├────────────────────────────────────────────────────────┤
│ 1. Unit Tests (`cargo test`)                           │
│    • Test domain logic, parser transforms, AST nodes   │
│ 2. Property-Based Tests (`proptest`)                   │
│    • Invariant fuzzing on Rope edits & CRDT merges     │
│ 3. Snapshot Tests (`insta`)                            │
│    • Regression validation for Typst/LaTeX compilers   │
│ 4. Fuzz Testing (`cargo-fuzz`)                         │
│    • Malformed input testing on WASM & parser bounds   │
│ 5. Performance Benchmarks (`criterion`)                │
│    • Microbenchmarks with strict latency SLAs          │
│ 6. End-to-End UI Tests (Playwright / Vitest)           │
│    • Verify SyncTeX bidirectional click jumps & canvas │
└────────────────────────────────────────────────────────┘
```

### 4.1. Unit & Property-Based Testing
* **Rope & Buffer Operations:** All buffer transformations must pass randomized property tests with `proptest`:
  ```rust
  proptest! {
      #[test]
      fn test_rope_random_inserts_and_deletes(ops in proptest::collection::vec(any::<BufferOp>(), 1..100)) {
          // Verify that state matches an oracle model and never panics
      }
  }
  ```
* **Snapshot Testing:** Use `insta` for all compiler AST outputs and diagnostic messages to prevent unintended regressions:
  ```rust
  #[test]
  fn test_cetz_parser_snapshot() {
      let ast = parse_cetz_snippet(SAMPLE_SNIPPET).unwrap();
      insta::assert_debug_snapshot!(ast);
  }
  ```

### 4.2. Performance Budgets & SLAs
Code changes that degrade benchmarks beyond these thresholds will be rejected:
* **Keystroke Input Latency:** $< 8\text{ ms}$ ($99^{\text{th}}$ percentile).
* **Typst Incremental Recompile:** $< 50\text{ ms}$ for documents under 50 pages.
* **Tectonic Cold Recompile:** $< 2.0\text{ s}$ (with cached CTAN bundles).
* **Application Idle Memory:** $< 75\text{ MB}$ total system RAM.
* **Cold App Launch Time:** $< 400\text{ ms}$ to interactive state.

---

## 5. Agent Verification Protocol (Before Submitting Any Work)

Before declaring any coding step or feature complete, an AI Agent MUST run the following quality gates in order:

1. **Rust Code Formatting:**
   ```bash
   cargo fmt --all -- --check
   ```
2. **Clippy Lints (Zero Warnings Policy):**
   ```bash
   cargo clippy --all-targets --all-features --locked -- -D warnings
   ```
3. **Test Suite Execution:**
   ```bash
   cargo test --all -- --nocapture
   ```
4. **Frontend Lints & Type Checks:**
   ```bash
   pnpm run typecheck && pnpm run lint
   ```
5. **Criterion Benchmark Regression Check (if touching core pipelines):**
   ```bash
   cargo bench --bench buffer_throughput
   ```

If any step fails, diagnose and fix the root cause. **Never disable lints or use `#[allow(clippy::...)]` without explicit justification and documented rationale.**

---

## 6. Git Hygiene & Commit Standards

* Follow **Conventional Commits**:
  - `feat(compiler): add incremental typst svg streaming`
  - `fix(canvas): prevent race condition on figure headless export`
  - `perf(buffer): optimize ropey line indexing via SIMD search`
  - `test(crdt): add property test for concurrent cross-client edits`
  - `docs(agents): update testing harness specifications`
* Every commit must be self-contained and leave the test suite passing.

---

## 7. Operational Hygiene (Leave No Mess Behind)

Agents (and humans) work in parallel on one machine with limited CPU and disk. A Rust build of this repo is 5-17 GB; abandoned worktrees and stray dev servers have cost hours. Follow these rules every time.

### 7.1. Processes and ports
* **Never leave a dev server, watcher, browser or fuzzer running** when you finish. Stop what you started.
* Before starting a server, check the port (`lsof -iTCP:<port> -sTCP:LISTEN`). The app's dev server is fixed at **1420** (Tauri `devUrl`); if something holds it, find out who. Never silently pick another port for `tauri dev`.
* Stop by **PID you started or by port listener** (`lsof -ti:1420 -sTCP:LISTEN | xargs -r kill`). Never `pkill -f` with a broad pattern: it can kill other sessions. Never kill processes you did not start.
* Fuzzers (`cargo-fuzz`) run with a hard cap (`-max_total_time=60` per target); longer runs belong in CI.
* Headless UI runs: `pnpm dev` then `node scripts/verify-ui.mjs`, then stop the server.

### 7.2. Worktrees and branches
* Parallel agents work in a **git worktree on their own branch** (`.claude/worktrees/<id>` or a sibling directory), owning disjoint files. `.claude/` is git-ignored and eslint/prettier-ignored.
* **When a branch is merged into `main`: remove the worktree and delete the branch**, in this order:
  `git worktree remove --force <path>` then `git branch -d <branch>` then `git worktree prune`.
  Only remove branches with `git rev-list --count main..<branch>` equal to 0.
* Never run `git add -A` before checking `git status`: embedded worktrees must stay untracked, build output must stay ignored.
* A worktree must start from the **current `main`** (`git merge main` or fast-forward first). Stale worktrees miss crates and cause conflicts.
* Automatic agent worktrees branch from `origin/main`, not local `main`. **Push `main` before spawning agents** (or create the worktree yourself with `git worktree add -b <branch> <path> main`), and have every agent verify `git log -1` matches `main` before starting.
* Merge conflicts in `Cargo.toml` (workspace `members`) and `Cargo.lock` are expected: keep the union of members and let Cargo regenerate the lockfile.

### 7.3. Builds and disk
* Do **not** run several heavy Rust builds at once (`cargo test --workspace`, `tauri dev`, and another agent's build share one `target/` lock and one CPU). Prefer `-p <crate>` while developing and the full workspace gate once at the end.
* Long commands run in the background with a timeout; wait for completion notifications instead of polling or sleeping.
* **Concurrency budget on one machine:** at most one heavy Rust build at a time (a workspace test run, `tauri dev`, or an agent's crate build), and at most one build-heavy agent alongside another tool such as Antigravity. Never run a full-workspace gate while agents are building. Watch `uptime`: a load average far above the core count means stop and serialise.
* Agents share the main build cache: `CARGO_TARGET_DIR=<repo>/target` with `-p <crate>`, instead of a fresh multi-GB `target/` per worktree.
* To wait for a process, wait on its PID (`while kill -0 <pid>; do ...`), never `pgrep -f "<its command text>"`: the waiting shell's own command line matches and the loop never ends.
* The root `Cargo.toml` optimises **dependencies** in dev builds (`[profile.dev.package."*"] opt-level = 3`) while our own crates stay at `opt-level = 1`. An unoptimised Typst is 10-50x slower and makes `tauri dev` unusable for judging speed. Keep it; judge performance only on this profile or release, never on a plain debug build.
* Delete build output you created in disposable worktrees (`cargo clean` or remove the worktree). Do not commit `target/`, `dist/`, `artifacts/` or fuzz corpora beyond minimal regression seeds.
* Scratch files go in the session scratchpad directory, never in the repo root or `/tmp`.

### 7.4. Dependencies and toolchain
* The npm `@tauri-apps/*` packages and the Rust `tauri*` crates must be on the **same major.minor**. Upgrade both together; `tauri dev` refuses to start on a mismatch.
* The workspace MSRV is `rust-version = "1.92"` (Tauri needs 1.90, typstyle-core 1.92). Crates inherit it with `rust-version.workspace = true`; do not pin lower values per crate, they force the resolver to older versions.
* Verify a crate version exists on crates.io before pinning it; prefer stable releases over release candidates.
* **Never link a crate that statically bundles a system library (HarfBuzz, ICU, FreeType, OpenSSL...) into the app binary.** Its symbols override the system copies GTK/WebKit use and crash the app at startup (this happened with Tectonic). Such engines run in a sidecar process behind a cargo feature (`manang-latex` feature `engine` builds `manang-latex-worker`). Check with `cargo tree -p manang | grep -iE "tectonic|harfbuzz"`, which must be empty.
* Do not change anything at system level (packages, system config, environment, files outside the repo and the app's own data dirs) as part of development work.

### 7.5. Plugin and UI conventions (so work merges cleanly)
* Every feature is a plugin crate under `crates/manang-<name>` exposing `manifest()` and `plugin(..)`, registered in `src-tauri/src/lib.rs`, with kernel-driven integration tests. Third-party plugins are WASM components (ADR 0003).
* Command ids are `<plugin>.<command>`; a plugin may only provide commands under its own name.
* UI vocabulary is fixed: **Files, Search, Figures, References, Plugins, Problems, Output, Settings**. Do not reintroduce Desk / Library Shelf / Instrument Shelf / Whispers.
* Clippy runs with let-chains enabled (edition 2024, MSRV 1.92): write `if let .. && ..` instead of nested `if let`.

