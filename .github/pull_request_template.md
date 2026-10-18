## Summary

<!-- Briefly describe the purpose, context, and non-obvious design decisions of this change. -->

## Changes

- 

## Verification Protocol (AGENTS.md Section 5)

All pull requests must pass the repository quality gates prior to merging:

- [ ] **Rust Formatting:** `cargo fmt --all -- --check`
- [ ] **Rust Clippy (Zero Warnings Policy):** `cargo clippy --all-targets --all-features --locked -- -D warnings`
- [ ] **Test Suite Execution:** `cargo test --all -- --nocapture` (or `-p <crate>` for localized changes)
- [ ] **Frontend Type & Lint Check:** `pnpm run typecheck && pnpm run lint`
- [ ] **Frontend Unit Tests:** `pnpm test`
- [ ] **UI Headless Verification (if touching UI):** `node scripts/verify-ui.mjs`
- [ ] **System Library Ban Audit:** `cargo tree -p manang | grep -iE "tectonic|harfbuzz|freetype-sys|icu4c"` returns empty
- [ ] **Criterion Benchmarks (if touching core pipelines):** `cargo bench --bench buffer_throughput`

## Operational Hygiene & Safety Invariants

- [ ] **Branch & Commit Hygiene:** Commits follow Conventional Commits (`feat:`, `fix:`, `perf:`, `docs:`, `test:`, `chore:`) and are developed on a feature branch (never directly on `main`).
- [ ] **No Unchecked Panics:** No `unwrap()`, `expect()`, `panic!`, or `todo!` in production modules; all fallible paths return explicit `Result`.
- [ ] **Process Cleanup:** No dev servers, background processes, or test listeners left bound to ports (e.g. 1420 or 1431-1443).
- [ ] **No Disk Waste:** Build artifacts, temporary folders, or scratch files outside `target/` are deleted.
