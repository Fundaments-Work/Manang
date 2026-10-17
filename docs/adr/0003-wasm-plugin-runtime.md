# ADR 0003: Host WASM plugins on the Wasmtime component model

Status: accepted, 2026-10-17

## Context

Every Manang feature is a plugin, in two tiers behind one `Plugin` trait
(`manang-kernel`): trusted native Rust for first-party hot paths, and sandboxed
WASM for third parties. AGENTS.md named Extism for the WASM tier.

Research for this decision found:

- WASI 0.3 (native async) was released on 11 June 2026. Wasmtime 46 is the first
  release implementing the final spec. Current stable is Wasmtime 49.
- Extism's Rust crate (1.30) lags Wasmtime, and its ABI is a byte/key-value
  memory interface that adds copying and hand-written serialisation.
- Typed WIT interfaces and `wit-bindgen` are the ecosystem direction.

## Decision

Host WASM plugins directly on **Wasmtime's component model** with our own WIT
package, `manang:plugin` (`crates/manang-wasm-host/wit/plugin.wit`). Drop Extism.

- **Capabilities are imports.** The host links only the interfaces a manifest
  grants. A component importing an ungranted interface is rejected at load
  (`WasmError::UngrantedImport`). Finer checks (read vs write) run in the host
  implementation of the interface.
- **No WASI.** Plugins get no ambient syscalls. Authors target
  `wasm32-unknown-unknown` and encode with `wit-component`.
- **Per-call isolation.** Each call gets a fresh `Store` with a memory cap
  (`Limits::memory_bytes`) and an epoch deadline (`Limits::deadline`). Plugins
  are stateless between calls; durable state goes through host services.
- **Pinned world.** We pin our own `manang:plugin@0.1.0` world and do not depend
  on WASI 0.3, because the guest toolchain still ships a release-candidate WIT.

## Consequences

- Plugin authors write against WIT instead of Extism's PDK. `wit-bindgen`
  supports Rust, and JS, Go and Python have component toolchains.
- Calls are synchronous and CPU-bound: run them on the Rayon pool.

## Follow-ups

- Precompile to cached `.cwasm` (`Engine::precompile_component`) for cold start.
- Evaluate the pooling allocator after measuring instantiation cost.
- Add filesystem and network interfaces, each gated like `buffer`.
- Fuzz the payload boundary (`cargo-fuzz`).
- Benchmark per-call overhead with `criterion` against the latency budget.
