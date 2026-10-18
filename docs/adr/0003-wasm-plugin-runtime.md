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
package, `manang:plugin` (`crates/manang-wasm-host/wit/plugin.wit`, now 0.2.0). Drop Extism.

- **Capabilities are imports.** The host links only the interfaces a manifest
  grants. A component importing an ungranted interface is rejected at load
  (`WasmError::UngrantedImport`). Finer checks (read vs write) run in the host
  implementation of the interface.
- **No WASI.** Plugins get no ambient syscalls. Authors target
  `wasm32-unknown-unknown` and encode with `wit-component`.
- **Per-call isolation.** Each call gets a fresh `Store` with a memory cap
  (`Limits::memory_bytes`) and an epoch deadline (`Limits::deadline`). Plugins
  are stateless between calls; durable state goes through host services.
- **Pinned world.** We pin our own `manang:plugin` world (0.2.0 as of the update below) and do not depend
  on WASI 0.3, because the guest toolchain still ships a release-candidate WIT.

## Consequences

- Plugin authors write against WIT instead of Extism's PDK. `wit-bindgen`
  supports Rust, and JS, Go and Python have component toolchains.
- Calls are synchronous and CPU-bound: run them on the Rayon pool.

## Update 2026-10-17: production hardening

Implemented (see `docs/plugins.md` for the author-facing view):

- **WIT bumped to 0.2.0.** Adds `fs` and `http` interfaces and worlds
  `plugin-fs`, `plugin-http`, `plugin-full`. A 0.1.0 component is refused with
  `WasmError::IncompatibleApi` instead of a bare "import not found".
- **`fs`** (`filesystem = ["read:<dir>", "write:<dir>"]`): paths are lexically
  validated, matched to the longest granted scope, then opened with `cap-std`
  beneath the scope directory, so `..` and symlink escapes (including races)
  cannot leave it. Per-file and per-call byte caps apply.
- **`http`** (`network = ["<host>"]`): exact-host allowlist checked for the
  request and every redirect hop (the client never follows redirects itself),
  no credentials in URLs, no ambient proxies, body, redirect and wall-clock
  caps. Built on `ureq` 3 (blocking, rustls).
- **Precompiled cache.** `.cwasm` artefacts keyed by
  `blake3(wasm) + engine fingerprint` (Wasmtime's own compatibility hash plus
  this crate's version), each carrying a keyed BLAKE3 MAC under a per-install
  secret. `Component::deserialize` is `unsafe`, so only MAC-verified bytes (or
  bytes this process just produced with `precompile_component`) reach it; that
  call is the crate's single `unsafe` block (`#![deny(unsafe_code)]` elsewhere).
  We deliberately read the file into memory and verify before deserialising
  rather than `deserialize_file`, which would let the file change after the
  check. Load of the echo test component drops from about 40 ms
  (compile) to about 0.2 ms (cache hit).
- **Pooling allocator: enabled by default** in `RuntimeOptions` (64 slots, with
  automatic fallback to on-demand if the address space cannot be reserved).
  Measured on the echo guest (criterion, release build): empty call 43 us
  on-demand vs 28 us pooled, 1 KB JSON 46 vs 29 us. `WasmRuntime::new` keeps
  on-demand allocation for compatibility.
- **Deadlines cover instantiation.** The epoch deadline is armed before
  instantiation, so a start function that spins is stopped. An epoch callback
  extends it only by time spent in host I/O, itself capped per call.
- **Caps**: payload in/out, error text, table elements, instances, memories,
  tables, per-component pool limits. Guest panics inside host services are
  caught at the call boundary (`catch_unwind`).
- **Kernel**: `Kernel::replace` swaps a plugin atomically for hot reload, with
  in-flight calls finishing on the old instance; `run_hook_each` stops one
  failing subscriber from hiding the rest of a fan-out hook.
- **`manang-plugins`** discovers, loads (compiling in parallel on Rayon),
  unloads and hot-reloads plugins; **`fuzz/`** holds cargo-fuzz targets for
  `Manifest::from_toml` and the payload boundary.

Known limits: `catch_unwind` has no effect under `panic = "abort"`, which the
workspace release profile sets; the shell should build with `panic = "unwind"`
if a host-side panic must not take the app down. Output from a guest is copied
once into host memory before the output cap is checked, so a hostile guest can
make the host copy up to `Limits::memory_bytes`, never more.

## Follow-ups

- Plugin signing. Practice in the WebAssembly ecosystem is an embedded or
  detached signature (the wasm module-signatures proposal, `wsc`/wasmsign2) or
  Sigstore keyless signing verified offline; verification belongs in the host,
  before load. Proposed: ed25519 publisher keys in the app config, a detached
  `plugin.sig` over `blake3(manifest) || blake3(wasm)`, and a prompt on first
  use of a new publisher.
- Per-plugin CPU accounting with fuel, to rank and throttle expensive hooks.
- Manifest-level `api = "0.2"` so incompatibility is reported before compiling.
- Per-call streaming for payloads larger than 16 MiB (shared buffers rather
  than copies).
- Revisit WASI 0.3 async once the guest toolchain ships its final WIT.
- Fuzz the `fs` path validator and the manifest-to-capability path with
  structure-aware inputs.

## Addendum 2026-10-18: stateful adapter sessions

Format adapters (ADR 0015) may hold one long-lived instance per open
document; commands and hooks stay stateless. See ADR 0015 section 6 for the
limits (memory cap, per-call deadline, 64 sessions per plugin), rebuild-on-trap
from the host rope, and the stateless fallback. `wit/format.wit` is the
interface.
