# A standalone, high-performance relay server

Date: 2026-10-20. Owner direction: write an extremely fast relay in Rust, in a
separate repository, with a proper library, real security testing, deployed to
the owner's Oracle Cloud VM. Source: a read-only design report on 2026-10-20
([V] verified in code or cited sources, [I] inferred, [U] unverified). Numbers
below are targets to measure, not promises.

## What the current relay (`crates/manang-relay`) lacks [V]

* One global `Mutex<HashMap>` for all rooms; every frame from every room
  serialises on it. Fine for a beta, wrong for a performance goal.
* Memory is bounded by frame count (512 per receiver), not bytes: a slow peer can
  pin gigabytes of shared buffers. tungstenite also assembles whole messages.
* The byte bucket holds two seconds of rate (8 MiB), so frames between 8 and 16
  MiB can never pass although the advertised cap is 16 MiB. Pick the real cap on
  purpose.
* Slow readers are only dropped when the queue fills; the idle timer resets on
  any inbound item, including a pong.
* No per-IP limits, no join-rate limits, no proxy-aware client IP, no metrics,
  no readiness or draining state. Frames are already shared with `Bytes` (no copy).
* The server imports protocol constants from `manang-live`, so it cannot move to
  its own repo without owning the wire contract.

## Choices

* **Runtime:** stay on tokio multi-thread. Thread-per-core (monoio, glommio,
  io_uring) only if the load generator shows the epoll path saturated. [I]
* **WebSocket:** a hyper upgrade with `fastwebsockets`, behind a trait, benchmarked
  against `tokio-tungstenite` (maintenance of fastwebsockets is [U]; check crates.io
  first). No permessage-deflate (useless on ciphertext, a bomb surface).
* **TLS:** terminate at Caddy on loopback first; in-process rustls on ring (no
  aws-lc, no C) later as a feature. Cloudflare in front adds DDoS absorption and
  hides the VM, but proxied WebSockets reportedly idle out at 100 s [U, community
  reports]: keep application keepalive configurable and test with real frames.
* **Concurrency:** a sharded room registry (64-256 shards), per-room state behind its
  own small lock, snapshot the sender list before sending, no global mutex; byte
  budgets per connection, per room and global.
* **Allocator:** mimalloc vs jemalloc, measured on both architectures (jemalloc
  and 64 KiB-page Arm kernels [U]); mimalloc contains C, so decide the "no C" rule.
* **Metrics and logs:** a hand-written Prometheus text endpoint on loopback; never
  log payloads, only connection id, a keyed hash of the room id, close reasons, sizes.

## New repository layout

`relay-proto` (envelope constants, room-id parsing, close codes, capability token
format; no dependency on Manang), `relay-core` (limits, sharded registry, budgets,
admission, server loop, metrics, `spawn(addr, config)` shaped like today),
`relay-client` (the `RelayClient`), `relay-server` (binary), `relay-loadgen`,
`fuzz/`, `tests/`. `WIRE.md` plus golden vectors; additions are negotiated by
upgrade parameters and new frame kinds, never by changing v1/v2 frame meaning.
Manang keeps its conformance suite and depends on the new crates pinned by
version; `manang-live` keeps the envelope types and a golden-vector test asserts
both repos agree.

## What a relay we run makes possible (honest limits)

* **Write tokens (M):** a signed token at upgrade lets the relay refuse frames from
  read-only connections before delivery. A viewer still holds the room key and can
  read; the relay cannot see authorship; tokens must be per invitee and short-lived.
* **Durable encrypted rooms (L, opt-in):** opaque snapshots and updates survive
  everyone leaving. The relay can truncate or roll back; clients embed a signed
  monotonic counter inside the ciphertext, which cannot protect a brand-new device.
  The relay stops being stateless: quotas, retention, deletion, backups.
* **Epoch / rekey frames (M relay, L client):** a new frame kind 3 carrying an epoch
  number and per-member wrapped keys; revocation stops future reads only.
* **Admission control (M):** per-IP connections and handshakes, join rate, rooms per
  IP, global byte budgets, proxy IP trusted only from configured addresses.

## Security testing

cargo-fuzz (HTTP handshake, WebSocket frame header and length, envelope plausibility,
token parse and verify, limiter state machine, durable-log reader); proptest
(bucket capacity, no negative budgets, registry leaks, no echo, room isolation);
hostile-client suite (slowloris, giant declared lengths, connection floods,
room-id squatting and enumeration, reconnect storms, half-open sockets, a client
that never reads, pong-only clients, text frames, bad close payloads) asserting
bounded memory and file descriptors; cargo-deny (ban openssl, aws-lc), cargo-audit
on a schedule, SBOM per release, `#![forbid(unsafe_code)]`, reproducible static
musl builds for x86_64 and aarch64, load generator run from another machine.

## Deployment on Oracle Cloud

* **Risk:** Oracle may reclaim an Always Free instance it deems idle (95th percentile
  CPU, network and, on A1, memory each under 20% for 7 days). A quiet relay meets
  that. Reported fix: upgrade to pay-as-you-go [U, third-party].
* Relay on `127.0.0.1:8080`, Caddy on 80/443. systemd with `DynamicUser`,
  `NoNewPrivileges`, `ProtectSystem=strict`, `MemoryMax`, `LimitNOFILE=65536`,
  restricted syscalls; sysctl drop-in (somaxconn, keepalive 60 s). Oracle security
  list opens only 22 (owner IP), 80, 443; the host firewall too.
* `/health` liveness and `/ready` that flips on SIGTERM; drain with Close 1012.
* Releases: tag builds both musl targets, SHA256SUMS, SBOM, cosign or minisign
  signature; `deploy.sh` verifies, installs to `/opt/manang-relay/<version>/`, flips
  a `current` symlink; rollback is the symlink flip.

## Work packages (at most one heavy build agent at a time, always capped)

| # | Package | Size | Heavy |
| :--- | :--- | :--- | :--- |
| W1 | Repo skeleton, `relay-proto`, golden vectors, CI, deny/audit | S | no |
| W2 | `relay-core`: sharded registry, byte budgets, bucket vs frame fix, metrics, compatible `spawn` | M | yes |
| W3 | Abuse controls, proxy trust, drain and ready | M | no |
| W5 | Load generator and benchmarks; decide runtime, WebSocket layer, allocator | M | yes, alone, other host |
| W4 | Binary, systemd, Caddy, deploy and rollback scripts | S | no |
| W6 | Fuzz targets, hostile suite, SBOM, reproducible signed builds | M | yes |
| W7 | Manang switches to the new crates; conformance against it and the Worker | S | yes |
| W8 | Write tokens and the client change | M | no |
| W9 | Durable rooms, then epoch frames | L | yes |

Order: W1, W2, W3, W5, W4, W6, W7, then W8, then W9; deploy a stateless relay after
W4 and W6.

## Needed from the owner

VM shape, CPU architecture, RAM, OS image, Always Free or pay-as-you-go; the domain
and DNS host for `relay.fundaments.work` and how Cloudflare is used today (is the
Worker bound to that name?); SSH method, user and sudo; public and reserved IP and any
existing security list; whether client IPs may be logged and for how long; the
repository name, licence and visibility; proxy choice and whether the Worker stays
as a fallback; the real maximum frame size; durable rooms yes or no with retention
and quota; the release signing scheme.

## Owner constraints (2026-10-20) and what they change

* **Cloudflare Tunnel, no Caddy.** `cloudflared` on the VM makes outbound-only
  connections to Cloudflare; the relay listens on `127.0.0.1` and speaks plain
  WebSocket. No TLS code in the relay at all, no inbound ports open (the Oracle
  security list can drop 80 and 443; SSH only from the owner's IP or through the
  tunnel). The client IP comes from `CF-Connecting-IP`, trusted only because the
  sole local peer is `cloudflared`. Cloudflare's edge gives DDoS absorption and
  can add rate-limit rules for free. The 100 s idle limit on proxied WebSockets
  stays: application keepalive uses real binary frames. The cut-over moves
  `relay.fundaments.work` from the Worker route to the tunnel hostname; keep the
  Worker on a second name as the fallback.
* **Target machine: Oracle Always Free AMD, 1 GB RAM, x86_64** (the E2 micro shape
  has a 1/8 OCPU baseline [I, confirm on the instance]). Goal: about 500 concurrent
  users, which is roughly 60-100 rooms of up to 16 peers. Capacity numbers are
  targets to measure with the load generator, not promises.
* **Memory is the hard budget.** Rough split [I]: OS and journald 250-300 MB,
  `cloudflared` 40-80 MB, relay at most 400 MB with a systemd `MemoryMax` of
  about 600 MB for the unit. For 500 connections that means at most about 300 KB
  per connection including queued frames, so: small read and write buffers (tune
  down from tungstenite's roughly 128 KiB each), byte budgets per connection, per
  room and **global** (a hard cap on in-flight bytes, for example 128 MiB), and
  slow readers dropped by bytes, not by frame count.
* **CPU is the second budget.** A 1/8 OCPU baseline favours few threads: start with
  `current_thread` or two tokio workers, batch writes (one flush per wake-up),
  no per-frame allocation, `Bytes` shared across receivers, no TLS, no compression.
  Release profile: fat LTO, `codegen-units = 1`, `panic = "abort"`, a conservative
  x86-64-v2 target (not `native`, the build host differs from the VM). The biggest
  CPU cost on the box may be `cloudflared`, not the relay: measure both, and
  compare `--protocol quic` against `http2`.
* **Frame cap decision:** keep the effective 8 MiB (what the code really allows
  today), make it an explicit documented constant, fix the bucket so the cap and
  the limiter agree, and let the global in-flight budget protect the box.
* **Oracle idle reclamation** applies to Always Free instances (CPU and network
  95th percentile under 20% for 7 days) [V, Oracle docs; remedy via pay-as-you-go
  is a third-party report, U]. Decide before depending on the VM; do not rely on
  fake load.
* A 1 GB machine has no swap by default. A small swap file or zram is the
  owner's call on their VM; the plan assumes none and relies on `MemoryMax`.
