# How Cloudflare optimises Rust for memory and speed, and what Manang can take from it

> Note (2026-10-20): The relay server moved to a separate private repository on 2026-10-20 (`crates/manang-relay` and `relay/worker` no longer exist here; the app keeps `crates/manang-relay-client`). Paths below describe the state when this was written.

Date: 2026-10-18. Status: research, no code changed. Method: primary sources only (Cloudflare blog posts and docs, the `cloudflare/pingora` repository), read on 2026-10-18. Where only a secondary source or nothing exists, the text says so. Figures are quoted as published; none are invented or extrapolated.

## Summary

1. Cloudflare's biggest recent memory wins are **data-layout changes in Rust, not allocator or compiler tricks**: two posts in 2026 each report about 100 TB of fleet RAM saved (the 1.1.1.1 DNS cache, 27 Aug 2026; the Pingora Backend Router hash rings, 18 Sep 2026).
2. The DNS cache work is a checklist we can copy almost verbatim: `Box<[T]>`/`Box<str>` instead of `Vec`/`String` for immutable data, `u16` offsets instead of extra lists, `Option<Box<_>>` for "same as the key", boxing the large variants of enums, and storing records as one wire-format byte buffer. Per-entry memory fell 953 to 420 bytes, and inserts and lookups got faster, not slower.
3. Pingora's win over NGINX (about 70% less CPU, about 67% less memory, connection reuse 87.1% to 99.92% for one customer) came from **one shared multithreaded pool instead of per-process pools**, `Arc`-shared state instead of copying, and Rust instead of Lua. It does not transfer directly to a desktop app, but the same principle applies to our relay: share, do not copy.
4. Cloudflare finds hot spots with **production sampling plus Criterion micro-benchmarks plus allocation counting (`dhat`, a custom wrapping global allocator)**, then rolls out with a dual-path flag so they can revert without a deploy. That workflow is cheap for us to adopt.
5. On allocators Cloudflare has published one primary data point: TCMalloc cut a RocksDB-backed service's memory by about 2.5x versus glibc malloc (2021). In the Rust services they describe in 2026, jemalloc is the production allocator (stated in the DNS post and offered by their `foundations` crate). They publish nothing comparing mimalloc.
6. For WASM, Cloudflare's published guidance is modest: `lto = true`, `strip = true`, `codegen-units = 1`, `wasm-opt` via `worker-build`. Our release profile already has these. The 2026 Workers limits are 128 MB per isolate, 64 MiB uncompressed bundle, 1 s startup.

### On the "about 1 TB saved" recollection

I could not find any Cloudflare primary source that reports about 1 TB saved. The primary sources report **100 TB**, twice, in 2026. The owner is most likely remembering one of these (the order of magnitude is off by 100x):

| Post | Date | Service | Reported saving |
| :--- | :--- | :--- | :--- |
| How we saved 100 terabytes of memory by optimizing 1.1.1.1's DNS cache | 27 Aug 2026 | Big Pineapple (1.1.1.1, Gateway DNS, DNS Firewall, AS112) | about 100 TB fleet-wide |
| Saving another 100TB of RAM with math (and Rust) | 18 Sep 2026 | Pingora Backend Router (PBR), `pingora-ketama` | about 100 TB fleet-wide |

A sub-figure inside the first post is the closest thing to a "TB-scale" single change: replacing `Vec`/`String` with `Box<[T]>`/`Box<str>` saved over 15 TB. Details follow in section 1.

---

## 1. Verified findings, by topic

### 1.1 The DNS cache rewrite (27 Aug 2026)

Source: <https://blog.cloudflare.com/dns-cache-memory-optimization-1111/> (no byline in the fetched text).

Context: Big Pineapple holds over 250 billion cache entries, so one wasted byte per entry costs over 250 GB fleet-wide.

Headline numbers (benchmark, then production):

| Metric | Before | After | Change |
| :--- | :--- | :--- | :--- |
| Memory per entry (benchmark) | 953 B | 420 B | -56% |
| Allocations per entry | 1.1 KB | 461 B | -58% |
| Insert throughput | 625,000 /s | 893,000 /s | +43% |
| Lookup latency | 828 ns | 670 ns | -19% |
| p99 instance memory (production) | 9.3 GB | 5.3 GB | -43% |
| p90 instance memory (production) | 6.5 GB | 3.8 GB | -42% |
| Fleet working set | | about 100 TB lower | |

Production savings were smaller than benchmark savings; the post says process memory also depends on traffic mix, occupancy, allocator state and non-cache memory. Rollout ran 18 May to 6 Jul 2026.

The five techniques, with the mechanism:

1. **`Box<[T]>` and `Box<str>` instead of `Vec`/`String`.** Entries are immutable after creation, so the capacity word (8 bytes) and any slack allocation are waste. Eight fields changed: 64 bytes per entry, over 15 TB fleet-wide.
2. **One shared record list plus `u16` section offsets** instead of separate answer/authority/additional lists. Two `Box<[T]>` fields (pointer plus length each) became two 2-byte offsets: 28 bytes per entry. Packing booleans into a bitflags type also cut padding.
3. **`Option<Box<Name>>` for the record owner.** `None` means "same as the query name", restored from the cache key at read time. Most records then need no owner allocation.
4. **Box the large enum variants.** `RecordData` was 144 bytes because of `NAPTR` (136 bytes), while an A record needs 4 bytes. Boxing `Txt`, `Naptr`, `Svcb` shrank the enum to 24 bytes (120 bytes saved per A/AAAA record). Stated costs: allocator size-class rounding (a 40-byte request lands in jemalloc's 48-byte bin) and worse locality.
5. **Wire-format storage.** Records live in one `Box<[u8]>` with a 2-byte length prefix per record. Only records containing domain names are parsed. Records are assembled in a reusable scratch buffer and copied into one exact-size allocation, which avoids shrinking a `Vec<u8>`. This step alone: lookup latency -5% and insert throughput +13% in benchmarks. Trade-off: no random indexing, which they judge negligible for 1-4 records per entry.

Measurement method (as published): a benchmark with randomly generated entries matching production mix (56% A, 25% AAAA, 19% TXT, 1-4 records per entry); a **custom global allocator wrapping `System` that counted allocations and bytes per entry**; throughput and latency measured over the whole cache flow so speed was not traded away; per-instance resident memory tracked during rollout. The service runs on jemalloc.

### 1.2 Pingora Backend Router hash rings (18 Sep 2026)

Source: <https://blog.cloudflare.com/saving-100-tb-of-ram-with-math/> (no byline in the fetched text; Kevin Guthrie and Mariia Iurchenko appear in the sidebar).

- Problem: `pingora-ketama` used up to about 6 GB on some nodes, because PBR keeps a separate hash ring per feature combination ("dozens" of rings) and Pingora defaulted to 160 hashes per server.
- Fix 1, maths: the coefficient of variation with k hashes per server is sqrt((N-1)/(N*k+1)). Returns diminish above roughly 10,000 to 100,000 hashes per weighted server. They cut generated hashes per server by **90%** with no appreciable error.
- Fix 2, layout: `Point { hash: u32, index: u32 }` is 8 bytes. Changing `index` to `u16` saves nothing because of alignment padding, so they used `struct Point([u8; 6])` with `from_ne_bytes` getters, avoiding `#[repr(packed)]`. That gave **-25%** memory for the structures.
- Rollout: v1 and v2 rings ran side by side and each request's hash picked the ring, so rollback needed no redeploy; staged by data centre to limit cache churn.
- Not published: per-node before/after memory and latency. The only fleet figure is about 100 TB.
- Shipped in `pingora-ketama` as an unadvertised cargo feature (`v2` ring with compact storage, faster sorting, configurable base hash count).

### 1.3 trie-hard: finding a 1.7% hot spot (10 Sep 2024)

Source: <https://blog.cloudflare.com/pingora-saving-compute-1-percent-at-a-time>

- Hot function: `clear_internal_headers` in `pingora-origin`, about 1.71% of the service's CPU, roughly 680 of 40,000 cores. It looped over 100+ header names calling `remove_header`.
- Found by **statistically sampling production stack traces** (a function's CPU share is the fraction of samples containing it).
- Criterion results: original 3.65 us; iterate the request's headers against a static set 1.53 us (2.39x); the `trie-hard` crate (tree in the bits of unsigned integers, one contiguous block) 0.93 us. Regex, FST and radix_trie were all slower than a hashmap.
- Production: 19/1111 samples (1.71%) before, 9/1103 (0.82%) with the hashmap, 4/1171 (0.34%) with trie-hard. Headline: over 500 cores freed; predicted saving 1.28 percentage points. In production since July 2024.

### 1.4 Allocation removal in bot management (19 Jun 2023, older but the clearest method write-up)

Source: <https://blog.cloudflare.com/how-cloudflare-runs-ml-inference-in-microseconds/>

Techniques: stack arrays instead of heap, pre-allocated buffers outside the hot path, reuse one string buffer across iterations, iterator-based in-place comparisons, borrowed inputs (`&[f32]`, `&[&str]`) in the FFI bindings instead of owned `Vec`/`String`, hash shared categorical features once. Results: P50 388 us to 309 us (-20%), P99 940 us to 813 us (-14%); a case-insensitive check about 40 ns to about 25 ns. Tool: the `dhat` crate as global allocator to **assert zero allocations on a hot path in tests** (it cannot see allocations made by C code across FFI).

### 1.5 Pingora design (2022 architecture, 2024 open-source release)

Sources: <https://blog.cloudflare.com/how-we-built-pingora-the-proxy-that-connects-cloudflare-to-the-internet/> (14 Sep 2022); <https://blog.cloudflare.com/pingora-open-source/> (28 Feb 2024); repository docs and manifests (read 2026-10-18).

- Result vs the old NGINX/Lua stack at the same traffic: about 70% less CPU, about 67% less memory ("a third of the CPU and memory").
- **Connection reuse**: NGINX pools per worker process, so more workers fragment reuse. Pingora is multithreaded with a pool shared across threads. One major customer's reuse ratio rose from 87.1% to 99.92%, new connections to its origins fell 160x; median TTFB -5 ms, p95 -80 ms. Across all customers: one third as many new connections per second.
- **Shared state through `Arc`**: NGINX shared memory needs a mutex on every access and only holds strings and numbers; Lua header access copies C data into fresh GC'd strings. Pingora shares most items via `Arc` references and reads strings directly.
- **Work stealing** was chosen on Tokio to avoid load imbalance and blocking problems. Pingora's config (`pingora-core` user guide) exposes `work_stealing` (default true) and `threads`. `pingora-runtime` also provides a **no-steal flavour**: N independent single-threaded Tokio runtimes, one per OS thread, with tasks pinned where spawned. The source comment says the multi-thread scheduler's work stealing carries overhead and the no-steal flavour aims to match single-thread efficiency while using multiple cores.
- **Pool sizing**: `upstream_keepalive_pool_size` is idle connections per Tokio worker; the maximum is that times `threads`. Reuse requires an identical peer (address, scheme, SNI, client cert, verification settings); a connection that errored is never reused; `idle_timeout = 0` disables pooling for a peer. No default pool size is documented.
- **Per-thread histograms have a memory cost** that Pingora documents: `runtime_metrics_poll_time_histogram` adds overhead to every task poll and its memory scales with runtimes x workers x buckets.
- **Lock-free read patterns**: `pingora-load-balancing` depends on `arc-swap = "1"` (manifest read 2026-10-18), and `tinyufo` (the Pingora cache crate) markets a lock-free design. Cloudflare's blog posts do not discuss arc-swap. The TinyUFO README (the project's own benchmark, 8 threads, x64 desktop) reports 148.7 M ops for pure reads versus 7.0 M for LRU and 14.1 M for moka, with a hit ratio of about 52.35% at 1% cache size (zipf 1); it also says memory use can exceed an LRU's for tiny assets. Treat those as vendor numbers.
- `pingora-core` depends on `bytes` and `parking_lot` (with `arc_lock`), plus `ahash`; jemalloc (`tikv-jemallocator`) appears there only as a dev-dependency. The workspace manifest has no release profile (only `[profile.bench] debug = true`), so Cloudflare leaves LTO and codegen settings to the embedding application.
- Graceful restart: the open-source `ecdysis` library (13 Feb 2026, <https://blog.cloudflare.com/ecdysis-rust-graceful-restarts/>) forks and execs a child that inherits the listener, in production since 2021 across 330+ data centres. The child starts with a fresh address space.

### 1.6 Allocators and heap profiling

- **TCMalloc vs glibc on RocksDB** (3 Feb 2021, <https://blog.cloudflare.com/the-effect-of-switching-to-tcmalloc-on-rocksdb-memory-use/>): the service grew from about 15 GB at start to about 30 GB over days. Heap profiling showed the live heap was roughly a third of RSS, so the loss was allocator fragmentation (60-80 glibc arenas for about 25 threads). TCMalloc gave a 2.5x reduction (the intro says "almost three times"; no post-switch GB figure is published). Conclusion: glibc malloc is a poor fit for long-running multithreaded services where memory matters; TCMalloc and jemalloc are better.
- **jemalloc** is the allocator named in the DNS post (size-class rounding discussed) and offered by Cloudflare's open-source `foundations` crate (24 Jan 2024, <https://blog.cloudflare.com/introducing-foundations-our-open-source-rust-service-foundation-library/>), which wraps jemalloc's profiling in a Rust API reachable through the built-in telemetry server. The post gives no procedure or numbers.
- **Workers heap profiler** (docs updated 10 Oct 2026, <https://developers.cloudflare.com/workers/observability/profiling-in-production/>): samples a stack every 512 kB allocated; measures allocations, not retained memory.
- Not found: any Cloudflare statement comparing jemalloc with mimalloc, or the allocator workerd uses. A third-party post (Svix, 4 Apr 2023, <https://www.svix.com/blog/heap-fragmentation-in-rust-applications/>) shows the same stair-step RSS pattern fixed by `jemallocator`, with graphs only and no figures.

### 1.7 Workers, workers-rs, WASM

- Limits (docs "Last updated Oct 12, 2026", <https://developers.cloudflare.com/workers/platform/limits/>): memory 128 MB per isolate (per isolate, not per invocation); Worker size 64 MiB uncompressed, no compressed limit; startup 1 second; CPU 10 ms per request on Free, 30 s default on Paid. Older third-party pages quote 3 MB / 10 MB sizes and a 400 ms startup; those are outdated.
- Rust guide (<https://developers.cloudflare.com/workers/languages/rust/>): template release profile is `lto = true`, `strip = true`, `codegen-units = 1`; `worker-build` runs `wasm-opt`. It warns that unoptimised binaries can exceed limits or start slowly, but gives no sizes.
- Panic strategy (22 Apr 2026, last modified 23 Sep 2026, <https://blog.cloudflare.com/making-rust-workers-reliable>): wasm32-unknown-unknown defaults to `panic=abort` (a trap). `panic=unwind` keeps the instance reusable but needs nightly `-Zbuild-std` and `extern "C-unwind"`, and workers-rs 0.8.0+ with `--panic-unwind`. Abort recovery reinitialises the instance and discards in-memory state. No size or speed numbers are published for either mode.
- Emscripten target (28 Sep 2026, <https://blog.cloudflare.com/rust-workers-emscripten-target/>): experimental `wasm32-unknown-emscripten` support, including Tokio via JSPI or a `LocalEventLoop`; no size, startup or memory numbers; pre-release patchsets.
- FL2 (the Rust proxy on Oxy that replaced NGINX/LuaJIT; 26 Sep 2025, <https://blog.cloudflare.com/20-percent-internet-upgrade/>, title on page "Cloudflare just got faster and more secure, powered by Rust"): "less than half the CPU" and "much less than half the memory" of FL1, no exact percentages; median response about 10 ms faster. The gain is attributed partly to removing C/Lua/Rust data conversions.

### 1.8 QUIC, Oxy, async overhead

- tokio-quiche (6 Nov 2025, <https://blog.cloudflare.com/async-quic-and-http-3-made-easy-tokio-quiche-is-now-open-source/>): actor model; an `InboundPacketRouter` owns the socket's receive half and routes datagrams by connection ID to a per-connection channel; one `IoWorker` actor per connection drives a quiche `Connection`. "Millions of HTTP/3 requests per second." The post announces future posts on UDP GSO/GRO, Tokio cooperative budgeting and mutexes, but **does not discuss zero-copy, backpressure or buffer management**, and gives no per-connection numbers.
- Oxy (<https://blog.cloudflare.com/introducing-oxy/>): an introduction; no memory-per-connection figures found.
- Not found: any Cloudflare post quantifying task-spawn cost or async-state-machine size, string interning, small-vector or arena techniques, or Cloudflare-published `cargo bloat`/`llvm-lines` results. The ROFL post (<https://blog.cloudflare.com/rust-nginx-module/>) mentions backpressure from downstream NGINX modules truncating responses, solved with a queue of unprocessed input buffers; it is about FFI, not general async.
- Cache transcoding (1 Sep 2026, <https://blog.cloudflare.com/cache-transcoding/>): zstd level 3 on cache fill, about a third of original on-disk size for eligible assets in a controlled corpus; encode 4.31 ns/byte, decode 1.56 ns/byte. Not memory-related; relevant only as a "pay once, serve many" trade-off.

---

## 2. What could not be verified

- The "about 1 TB" figure (see the top). Closest verified: 15 TB (the `Vec` to `Box<[T]>` step alone) and 100 TB (each of the two 2026 posts).
- Pingora memory per connection: no figure is published.
- Any allocator comparison involving mimalloc from Cloudflare; the allocator inside workerd.
- Cloudflare-side binary size or cold-start numbers for Rust Workers (only qualitative guidance).
- Whether Pingora's no-steal runtime is what Cloudflare runs in production; the code and docs only show it exists and is selectable.
- The real-world benefit of `arc-swap` in Pingora: only the dependency is verified.
- The two DNS/PBR posts have no byline in the fetched text; author attribution from secondary sources was not relied on.
- Date caveat: some posts list a later `dateModified` than publication; the dates above are publication dates.

---

## 3. Translation to Manang

Baseline in our workspace (read from the repo): root `Cargo.toml` release profile already has `codegen-units = 1`, `lto = true`, `opt-level = 3`, `panic = "abort"`, `strip = true`; dev profile optimises dependencies. No crate sets a global allocator, uses `dhat`, `arc-swap`, `smallvec` or string-interning crates. Criterion benches exist in about thirteen crates (including `manang-buffer`, `manang-live`, `manang-typst`, `manang-editor`, `manang-wasm-host`). `manang-relay` uses `tokio` (`rt-multi-thread`), `parking_lot`, `Bytes` frames, a single `Mutex<HashMap<RoomId, Room>>`, a per-peer `mpsc::Sender<Bytes>` with a bounded queue, a `Semaphore` for `max_connections` (4096), and `max_frame_bytes` of 16 MiB. Gain estimates below are **hypotheses to be measured**, not promises; where Cloudflare numbers back them I cite the section.

Priority: P0 = measure first / cheap and likely, P1 = worth doing after a measurement shows a need, P2 = speculative.

### (a) Desktop app (idle RSS under 75 MB, keystroke under 8 ms, Typst incremental under 50 ms)

| Pri | Technique | Expected gain | Cost | How to measure | Where |
| :--- | :--- | :--- | :--- | :--- | :--- |
| P0 | Build a **counting global allocator test harness** (Cloudflare's wrapped `System` allocator, 1.1; `dhat` assertions, 1.4) and add "zero allocation per keystroke on the hot path" tests | Finds hidden allocs; Cloudflare saw -20% P50 from removing them. Ours unknown | Low (test-only) | `dhat` in a `#[test]` or bench; assert allocation counts per edit; criterion for time | `manang-buffer`, `manang-editor`, `manang-live` |
| P0 | **Idle memory attribution**: record RSS and allocator stats at idle on a representative project, then heaptrack/dhat the startup | Tells us where the 75 MB goes before any tuning | Low | `heaptrack` run of the app; `/proc/<pid>/smaps_rollup`; jemalloc stats if enabled | `src-tauri`, `manang-kernel`, `manang-plugins` |
| P1 | **Allocator A/B** (jemalloc vs mimalloc vs system) on the desktop binary behind a cargo feature | Cloudflare's 2021 result (2.5x) is for a many-arena glibc service; on a desktop idle process the win is likely small and may raise idle RSS (jemalloc keeps size classes and arenas). Unknown | Low to try, medium to maintain; AGENTS 7.4 forbids statically bundled system libs, and jemalloc/mimalloc bundle C code, so confirm they do not collide with GTK/WebKit symbols before shipping | Same scripted session under each allocator: RSS at idle, RSS after opening a 50-page doc, p99 keystroke latency | `src-tauri` feature flag |
| P1 | **Shrink long-lived immutable structures** with the DNS-cache checklist (1.1): `Box<[T]>`/`Box<str>`, `u16` offsets, `Option<Box<_>>`, boxed large enum variants, compact `Point`-style structs | In Cloudflare's case -56% per entry. Applies where we hold many small immutable records: citation index, search index, syntax/AST nodes, figure scene nodes | Medium (API churn) | `std::mem::size_of` assertions in tests (cheap guard against regressions) plus counting-allocator test per record | `manang-citations`, `manang-search`, `manang-figure`, `manang-markdown` |
| P1 | **Sample production-like profiles** to find 1%-class hot functions (1.3): `perf record` or `samply` flamegraph during a scripted typing session; fix the top few | Cloudflare found a 1.7% function this way | Low | `cargo flamegraph` / `samply`; criterion before/after | `manang-editor`, `manang-typst` |
| P1 | **Typst recompile**: avoid copying sources/outputs across the IPC and compile boundary; reuse scratch buffers (1.1 step 5, 1.4) | Not predictable; target is to stay under 50 ms | Medium | `manang-typst` criterion bench (exists) with an allocation counter | `manang-typst`, `src-tauri` |
| P2 | `cargo bloat` and `cargo llvm-lines` to pull generic bloat out of the app crate | Smaller binary and faster cold start; magnitude unknown | Low | `cargo bloat --release --crates`; `cargo llvm-lines -p <crate>` | all, start with `src-tauri` |

### (b) Relay (many idle WebSockets on a 1 GB VM)

Budget arithmetic to make explicit: 1 GB divided by `max_connections = 4096` leaves about 250 KB per connection, minus runtime and OS buffers. The real number is unmeasured, so measure first.

| Pri | Technique | Expected gain | Cost | How to measure | Where |
| :--- | :--- | :--- | :--- | :--- | :--- |
| P0 | **Measure memory per idle connection**: open N idle sockets (N = 1k, 4k) with a load tool, read RSS delta | Gives the baseline Cloudflare never published; everything else depends on it | Low | a small Rust/Python soak client; RSS from `/proc`; `heaptrack` on the relay | `manang-relay` (add a `tests/` soak or an example) |
| P0 | **Allocator**: try jemalloc or mimalloc in the relay binary only (no GTK there, so AGENTS 7.4's symbol-collision concern does not apply) and compare RSS at 4k idle sockets and after churn | Cloudflare's published case is fragmentation over days with many arenas (1.6). A multithreaded relay with connect/disconnect churn is exactly that shape. Gain unknown until measured | Low | RSS over a 1-hour churn soak under each allocator; `MALLOC_ARENA_MAX` as a third arm | `manang-relay` (cargo feature) |
| P0 | **Cap per-connection buffers**: `outbound_queue` depth x frame size is the worst case per peer; make the product explicit and add a per-room byte budget. `max_frame_bytes` of 16 MiB x 4096 connections is a theoretical 64 GiB | Prevents OOM on 1 GB; backpressure already exists via bounded `mpsc` and the "dropped as too slow" rule | Low | test with slow consumers; heaptrack peak | `manang-relay` `config.rs`, `server.rs` |
| P1 | **Reduce lock scope**: the `Mutex<HashMap<RoomId, Room>>` is one lock for all rooms. Shard by room (or `Arc<Room>` with its own lock) so a hot room does not stall joins. `arc-swap` fits read-mostly config, not this map. Pingora's lesson is "share via `Arc`, keep critical sections tiny" (1.5) | Latency under load; memory neutral | Medium | criterion bench with N rooms x M peers broadcasting; `tokio-console` or contention profile | `manang-relay` |
| P1 | **Zero-copy fan-out**: frames are already `Bytes`; confirm one frame is cloned by refcount to N peers and not copied (check `tokio-tungstenite` message conversion) | Removes N copies per broadcast | Low to verify | dhat bytes-allocated per broadcast in a test | `manang-relay` |
| P1 | **Runtime config**: try `worker_threads` = 1 or 2 on a small VM, and Pingora's no-steal pattern (N current-thread runtimes) if profiling shows scheduler overhead. Cloudflare only says the no-steal flavour exists to avoid work-stealing overhead; no numbers | Fewer threads means fewer arenas and stacks; small | Low | RSS and p99 latency at 4k sockets with 1, 2, 4 threads | `manang-relay` `main.rs` |
| P1 | **Idle timeout and keep-alive policy**: the relay already pings; make idle and dead-peer timeouts explicit and tested (the Pingora pool lesson is that reuse and expiry rules dominate resource use) | Reclaims dead sockets | Low | soak test with half-open peers | `manang-relay` |
| P2 | Dual-path rollout flag for risky layout changes (1.2) | Safe rollback without redeploy | Low | n/a | relay config |

### (c) Plugin host and WASM plugins (Wasmtime)

| Pri | Technique | Expected gain | Cost | How to measure | Where |
| :--- | :--- | :--- | :--- | :--- | :--- |
| P0 | **Per-plugin memory and instance cost**: measure RSS per instantiated component and time to instantiate; enforce `StoreLimits` (memory, tables, instances) from manifest capabilities | Bounds the worst case; Cloudflare uses 128 MB per isolate and a 1 s startup limit as hard caps (1.7), a useful precedent | Low | criterion bench of instantiate + first call; `heaptrack` | `manang-wasm-host`, `manang-plugins` |
| P1 | **Lazy plugin instantiation and instance reuse**: compile once (`Engine`/`Component` cached, serialised to disk with `Engine::precompile_component`), instantiate on first command | Idle RSS and cold start | Medium | idle RSS with 0 vs all plugins loaded; startup time | `manang-wasm-host`, `manang-plugins` |
| P1 | **Binary IPC for plugin payloads**: pass byte slices and borrow where the component model permits, avoid re-serialising large data (Cloudflare 1.4: borrowed inputs at the FFI edge) | Fewer copies per call | Medium | dhat bytes per call; criterion | `manang-wasm-host` |
| P1 | **Plugin build profile guidance** for first-party WASM plugins: `opt-level = "s"` or `"z"`, `lto`, `codegen-units = 1`, `strip`, then `wasm-opt -Os` (matching Cloudflare's template, 1.7) | Smaller artifacts; Cloudflare gives no numbers, so we measure | Low | file size before and after; `twiggy`/`cargo bloat --target wasm32` | plugin crates, `docs/plugins.md` |
| P2 | `panic=abort` stays our default; recovery strategy for a trapped plugin is to discard the instance and re-instantiate (this mirrors Cloudflare's abort recovery, 1.7). Do not adopt `panic=unwind` for WASM plugins (needs nightly `-Zbuild-std`) | Simplicity | Low | fault-injection test | `manang-wasm-host` |

### (d) Future Rust-to-WASM web build (browser and Workers)

| Pri | Technique | Expected gain | Cost | How to measure | Where |
| :--- | :--- | :--- | :--- | :--- | :--- |
| P0 | **Size budget in CI** for the `wasm32-unknown-unknown` artifact: record `.wasm` raw and gzip/brotli size per crate that must compile there (AGENTS 2.5), and fail on growth above a threshold | Prevents silent bloat; Workers accepts 64 MiB uncompressed (1.7), browsers care about transfer size far more | Low | `cargo bloat --release --target wasm32-unknown-unknown --crates`; `twiggy top`; `wasm-opt -Oz` before/after | web-capable crates: `manang-buffer`, `manang-live`, `manang-markdown`, `manang-formats` (confirm which compile to wasm32) |
| P0 | Use the same release profile Cloudflare ships (`lto`, `codegen-units = 1`, `strip`, `wasm-opt`), plus `opt-level = "s"` as an experiment | Typically a large size reduction versus defaults; we have no Cloudflare number, so measure | Low | size delta | web build profile |
| P1 | **Keep OS-only dependencies out** of web-capable crates (already an invariant) and put clock, fs and network behind traits; run `cargo llvm-lines` on generic-heavy crates | Smaller wasm; fewer monomorphised copies | Medium | `cargo llvm-lines -p manang-live --lib` | `manang-live`, `manang-live-plugin` |
| P1 | **Layout discipline for in-browser documents**: the CRDT and rope structures dominate browser memory; apply the checklist in 1.1 | Lower tab memory; a 128 MB Worker limit makes this concrete if we run relay logic on Workers | Medium | `wasm` heap size via `memory.buffer.byteLength` in a vitest run on a large doc | `manang-live`, `manang-buffer` |
| P2 | Worker-hosted relay logic: respect 128 MB per isolate and 1 s startup (1.7); avoid `panic=unwind`; wasm-bindgen Emscripten target is experimental (1.7), so do not depend on it | n/a | n/a | n/a | future `manang-relay` web variant |

---

## 4. Performance budget and measurement plan (consistent with AGENTS.md section 4.2)

Existing budgets (AGENTS 4.2): keystroke latency under 8 ms p99; Typst incremental recompile under 50 ms for documents under 50 pages; Tectonic cold recompile under 2.0 s; idle memory under 75 MB; cold launch under 400 ms. Proposed additions are marked NEW; thresholds for NEW items are placeholders to be set after the first baseline run, because no baseline exists yet.

| What | Metric and budget | Tool | When | Gate |
| :--- | :--- | :--- | :--- | :--- |
| Keystroke path | p99 < 8 ms; NEW: 0 heap allocations per plain-character insert in `manang-buffer`/`manang-editor` | criterion + `dhat` assertion test | every PR touching those crates (quick), full nightly | CI fail if criterion regresses beyond noise threshold (suggest 10%) or the alloc count rises |
| Typst recompile | < 50 ms incremental (<50 pages) | `manang-typst` criterion | PR touching `manang-typst`; nightly | same regression rule |
| Idle desktop RSS | < 75 MB | scripted launch, read `smaps_rollup` after 30 s idle (Linux CI runner or local) | weekly and before release | fail above 75 MB |
| Cold launch | < 400 ms to interactive | existing UI verify script plus timing | weekly and before release | fail above 400 ms |
| Relay idle connection cost (NEW) | KB per idle socket at 1k and 4k; target: 4096 sockets under about 600 MB RSS on the 1 GB VM (placeholder) | soak client plus `/proc` RSS; `heaptrack` for attribution | before each relay release; after any `manang-relay` change to buffers or locking | fail if RSS exceeds the placeholder after baseline is set |
| Relay broadcast throughput and p99 (NEW) | frames/s and p99 fan-out latency with M peers x N rooms | criterion in `manang-relay` | PR touching relay | regression threshold 10% |
| Relay churn soak (NEW) | RSS after 1 h of connect/disconnect churn minus RSS at start (fragmentation check, 1.6) | soak script, allocator A/B | monthly and on allocator changes | informational, then gate |
| Struct size guards (NEW) | `size_of` asserts for hot record types (cheap, deterministic, mirrors Cloudflare's `Point` and `RecordData` work) | unit tests | every PR (runs in `cargo test`) | fail on growth |
| Plugin cost (NEW) | instantiate time and RSS per plugin; memory cap enforced | `manang-wasm-host` criterion | PR touching host | regression rule |
| WASM size (NEW) | `.wasm` raw and brotli size per web-capable crate | `cargo bloat`, `twiggy`, `wasm-opt -Oz` | PR touching those crates; weekly trend | fail on growth over a set percentage |
| Binary and compile bloat | `cargo bloat --release --crates` top-10 and `cargo llvm-lines` top functions | cargo-bloat, cargo-llvm-lines | monthly review, not a gate | none |
| Production-style profile | flamegraph of a scripted editing session | `cargo flamegraph` or `samply` | monthly and before a perf-labelled release | none; feeds the P1 backlog |

Practical rules taken from how Cloudflare works:

- Count first (allocation counters, `size_of`), then optimise; keep the counter as a regression test.
- Benchmarks should use input shaped like real traffic (their DNS benchmark used the production record-type mix), not uniform data.
- Roll out risky layout changes behind a switch so they can be reverted without a deploy.
- Judge memory in a steady-state run, not just a cold start: their production savings were smaller than their benchmark savings.
- Respect AGENTS 7.3: only one heavy build at a time; heaptrack, flamegraph and soak runs are run alone on the machine; benchmarks run with the dev-dependency-optimised or release profile, never plain debug.

## 5. Suggested first three steps

1. Add `dhat` allocation-count tests and `size_of` guards in `manang-buffer` and `manang-editor`, and a relay idle-socket soak measurement (cost: about a day, no production risk).
2. Run the relay allocator A/B (system vs jemalloc vs mimalloc) on the 1 GB VM profile; the relay has no GTK constraint.
3. Add the wasm size report to CI for the crates that must compile to `wasm32-unknown-unknown`.

## Sources

Cloudflare blog and docs (all fetched 2026-10-18):

- How we saved 100 terabytes of memory by optimizing 1.1.1.1's DNS cache, 27 Aug 2026: <https://blog.cloudflare.com/dns-cache-memory-optimization-1111/>
- Saving another 100TB of RAM with math (and Rust), 18 Sep 2026: <https://blog.cloudflare.com/saving-100-tb-of-ram-with-math/>
- A good day to trie-hard: saving compute 1% at a time, 10 Sep 2024: <https://blog.cloudflare.com/pingora-saving-compute-1-percent-at-a-time>
- How Cloudflare runs machine learning inference in microseconds, 19 Jun 2023: <https://blog.cloudflare.com/how-cloudflare-runs-ml-inference-in-microseconds/>
- How we built Pingora, 14 Sep 2022: <https://blog.cloudflare.com/how-we-built-pingora-the-proxy-that-connects-cloudflare-to-the-internet/>
- Open sourcing Pingora, 28 Feb 2024: <https://blog.cloudflare.com/pingora-open-source/>
- Shedding old code with ecdysis, 13 Feb 2026: <https://blog.cloudflare.com/ecdysis-rust-graceful-restarts/>
- Cache transcoding with Zstandard and Pingora, 1 Sep 2026: <https://blog.cloudflare.com/cache-transcoding/>
- The effect of switching to TCMalloc on RocksDB memory use, 3 Feb 2021: <https://blog.cloudflare.com/the-effect-of-switching-to-tcmalloc-on-rocksdb-memory-use/>
- Introducing foundations, 24 Jan 2024: <https://blog.cloudflare.com/introducing-foundations-our-open-source-rust-service-foundation-library/>
- Cloudflare just got faster and more secure, powered by Rust (FL2), 26 Sep 2025: <https://blog.cloudflare.com/20-percent-internet-upgrade/>
- tokio-quiche is now open source, 6 Nov 2025: <https://blog.cloudflare.com/async-quic-and-http-3-made-easy-tokio-quiche-is-now-open-source/>
- Making Rust Workers reliable, 22 Apr 2026: <https://blog.cloudflare.com/making-rust-workers-reliable>
- Native Rust in Workers with the Emscripten target, 28 Sep 2026: <https://blog.cloudflare.com/rust-workers-emscripten-target/>
- Replacing an NGINX module with ROFL: <https://blog.cloudflare.com/rust-nginx-module/>
- Introducing Oxy: <https://blog.cloudflare.com/introducing-oxy/>
- Workers limits (updated 12 Oct 2026): <https://developers.cloudflare.com/workers/platform/limits/>
- Workers Rust guide: <https://developers.cloudflare.com/workers/languages/rust/>
- Profiling in production (Workers, updated 10 Oct 2026): <https://developers.cloudflare.com/workers/observability/profiling-in-production/>

Pingora repository (<https://github.com/cloudflare/pingora>, `main`, read 2026-10-18): `pingora-core/Cargo.toml`, root `Cargo.toml`, `pingora-load-balancing/Cargo.toml`, `pingora-runtime/src/lib.rs`, `docs/user_guide/pooling.md`, `docs/user_guide/conf.md`; TinyUFO README and docs.rs page (vendor benchmarks, not independently reproduced).

Third party (lower weight): Svix, Heap fragmentation in Rust applications, 4 Apr 2023: <https://www.svix.com/blog/heap-fragmentation-in-rust-applications/>. InfoQ's Sep 2026 write-up of the DNS post was seen only in search results and not relied on.
