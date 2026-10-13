# AGENTS.md — Development Guidelines & Operational Rules for AI Agents

**Project:** Manang (The High-Performance Scientific Authoring & Illustration Suite)  
**Target:** AI Coding Agents (Antigravity, Cursor, Claude Code, Copilot, etc.) and Human Engineers  
**Version:** 1.0.0  
**Last Updated:** 2026-10-13  

---

## 1. System Identity & Mission

Manang is an uncompromisingly fast, local-first scientific authoring and illustration suite combining **Typst**, **LaTeX (Tectonic)**, **Markdown**, **Excalidraw (MIT)**, and a **CeTZ / TikZ Visual Studio**. 

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
4. **WASM Sandboxing for Extensions:** Third-party plugins must NEVER be granted direct OS syscall access. They must execute inside **Extism / Wasmtime** sandboxes with explicit capability declarations in `manifest.toml`.
5. **Local-First & Git-Native:** Documents must remain human-readable UTF-8 files on disk. Never introduce proprietary binary database formats that trap user files.

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
| **State Management** | Keep state close to the leaf components. Use immutable CodeMirror 6 transactions and Yjs docs. | Do not create massive monolithic React context providers that cause whole-page re-renders. |
| **Canvas Integration** | Use Excalidraw's official MIT component. Save raw `.excalidraw` scenes alongside headless-rendered vector SVGs. | Do not pull in proprietary or source-available canvas SDKs that require commercial licenses or inject watermarks. |
| **Styling & Assets** | Use Tailwind CSS with CSS variable design tokens and accessible `radix-ui` / `shadcn/ui` primitives. | Do not hardcode arbitrary hex colors or inject unstyled inline CSS blocks. |
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
  - `fix(canvas): prevent race condition on excalidraw headless export`
  - `perf(buffer): optimize ropey line indexing via SIMD search`
  - `test(crdt): add property test for concurrent cross-client edits`
  - `docs(agents): update testing harness specifications`
* Every commit must be self-contained and leave the test suite passing.
