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

## Research results and the merged plan (2026-10-20, second round)

Sources: two read-only reports (a memory- and CPU-minimal native design; a
Cloudflare-compatible design with pricing), plus checks by the lead. [V] verified,
[I] inferred, [U] unverified.

### Decided direction

* **Production target (owner):** the powerful native relay on the Oracle VM behind
  the Cloudflare Tunnel, served as `relay.fundaments.work`; `relay-test` is
  temporary and is deleted after the cut-over. **Cloudflare stays a ready second
  backend** (the existing Worker, brought to parity), not a rewrite.
* **Architecture: two backends, one contract ("D+").** A shared `relay-proto`
  (done) plus a sans-IO core crate that holds only per-room and per-connection
  decisions (path and frame checks, token bucket, room state machine, capability
  verification, store effects) and compiles to wasm32 (CI check). The native
  adapter owns the registry, budgets, timers and sockets; the Durable Object
  adapter owns hibernation. Golden vectors plus one `relay-conformance` binary
  (`--url`, advertised capabilities) prove equivalence on native, `wrangler dev`
  and production. A workers-rs spike is optional and last (the SDK's hibernation
  support exists but the Rust page documents none [V/I]).

### Verified facts

* tungstenite 0.30 defaults: read buffer 128 KiB, write buffer 128 KiB, max message
  64 MiB, max frame 16 MiB [V, source]. At 500 connections that is the largest
  controllable memory cost, so it must be tuned.
* A cgroup v2 memory cap includes kernel socket buffers (`sock` in `memory.stat`)
  [V, LWN]; per-socket `SO_SNDBUF`/`SO_RCVBUF` bound them (setting them disables
  autotuning; the kernel doubles the value).
* The VM (measured): Ubuntu 24.04, EPYC 7551 (AVX2 and BMI2 present), 954 MB RAM,
  about 500 MB available. `snapd` hosts Oracle's cloud agent, so do not remove it.
  `containerd` and `runc` (34 MB) are leftovers of a Docker attempt with nothing
  depending on them. `fwupd` (42 MB) is useless on a VM. `multipathd` (26 MB): root
  is plain ext4, no iSCSI sessions; decide after checking `multipath -ll`.
  `check-new-release` (97 MB) is a login-time MOTD hook.
* Cloudflare, 500 users doing live co-editing (0.7 billed messages/s/user, the 20:1
  rule) [I from docs]: the Free plan cannot carry it (100k Durable Object
  requests/day lasts hours); the Paid plan is about $7/month at 500 users and
  about $22/month at 5,000 for requests; duration cost depends on rooms
  hibernating (worst case, never hibernating, about $58/month at 500 users:
  measure before promising). Durable storage must batch writes (per-frame rows
  would cost about $170/month at 500 users). Cloudflare Containers is a poor
  elastic fit (about $12/month always-on plus the plan fee, single region).
* A Worker Custom Domain cannot coexist with a CNAME on the same hostname [V], so
  moving `relay.fundaments.work` from the Worker to the tunnel is a short,
  announced switch: delete the Custom Domain binding, create the tunnel hostname.
  Keep the Worker on a second name as the fallback. Invites carry the relay
  address, so the test hostname must never be put in invites.
* Today's contract drift [V]: the Worker never pings and idles whole rooms at 15
  minutes, answers 429/426, advertises 16 MiB; native pings every 25 s, idles per
  connection at 90 s, effective cap 8 MiB, sends 1013 for slow readers. The
  vectors and X3 below remove this drift.
* Through the tunnel today: `https://relay-test.fundaments.work/health` returns
  `ok`, a valid room upgrade returns `101 Switching Protocols`. On the VM the
  relay answers a bad room path with a clean 404; any other unknown path gets an
  empty reply instead of an HTTP error (fix in the new server).

### Native design (targets, not measurements)

* One tokio `current_thread` runtime (a 1/8-OCPU baseline gains nothing from
  work stealing); benchmark one against two workers.
* Memory budget at 500 connections about 160-175 MB: about 12-16 KiB idle per
  connection (4 KiB read buffer, no write buffer, `SO_SNDBUF` about 64 KiB and
  `SO_RCVBUF` about 32 KiB, `TCP_NODELAY`), a global in-flight frame budget of
  96 MiB counted once per shared allocation, admission by declared frame length
  before the body is read, bulk frames over 1 MiB behind at most 4 permits,
  refuse new connections above 85% of `memory.current`, `MemoryMax` only a backstop
  (no `MemoryHigh` without swap).
* Fan-out: sharded registry (64 shards, `[u8;16]` keys, no lock across an await or
  syscall), direct send from the sender's task, per-peer byte-budgeted queue with
  an age limit, one `Bytes` allocation per frame, vectored writes (up to 256 KiB
  per wake-up), one shared 1 s sweeper for pings and idle instead of timers per
  connection. Slow reader policy by bytes and age; presence is lossy (latest
  wins); data frames are never silently dropped (disconnect with 1013, the client
  resyncs). Shed order: refuse connections, drop presence, refuse new rooms, refuse
  frames over 1 MiB, disconnect the largest queues.
* WebSocket layer behind a `WsLayer` trait: tuned tokio-tungstenite first
  (`read_buffer_size` 4 KiB, `write_buffer_size` 0, explicit limits). A small custom
  RFC 6455 codec (about 300 lines, `httparse` for the handshake, fuzzed with a
  differential oracle against tungstenite, mask required, RSV bits rejected so
  deflate stays off) is built only if tuned tungstenite exceeds 40 KiB per
  connection or 5% of one core at the target mix. tokio-tungstenite 0.30.0
  (2026-07-11) is maintained; `fastwebsockets` last released 2025-01-14 and is not
  recommended.
* Allocator: glibc with `MALLOC_ARENA_MAX=1`, `MALLOC_MMAP_THRESHOLD_=262144`,
  `MALLOC_TRIM_THRESHOLD_=262144` in the env file first; measure jemalloc and
  mimalloc after churn before changing. Release profile as in `Cargo.toml`;
  `x86-64-v2` by default.
* Behind `cloudflared`: relay keepalive 25-30 s (the edge idles proxied WebSockets
  at roughly 100 s [U, community]), `CF-Connecting-IP` only from loopback, no TLS,
  `GOMEMLIMIT` for cloudflared, and a measured choice of QUIC versus HTTP/2 and of
  2 versus 4 tunnel connections.
* Pass criteria for 500 users (from a separate host, after 30 minutes of sustained
  load): `memory.current` of the relay at most 200 MB under the full mix and 150
  MB idle, p99 forward latency at most 50 ms (250 ms during bulk sync), relay CPU
  at most 5% of one vCPU on average, relay plus cloudflared at most 12%, RSS back
  within 30 MB after everyone disconnects, no descriptor leak, under 5% growth in a
  one-hour soak, and the hostile suite stays inside budgets.

### Work packages (at most one heavy build agent at a time, always capped)

| # | Package | Heavy |
| :--- | :--- | :--- |
| N1 | `relay-core`: sans-IO per-room/per-connection logic, sharded registry, byte-budgeted queues, global budget type, proptests, wasm32 check | no |
| X2 | `relay-conformance` runner, `/.well-known/relay` capabilities, vectors for HTTP codes, limiter traces, 150 s idle survival | no |
| N2 | `WsLayer` trait and tuned tokio-tungstenite implementation | no |
| N3 | Server loop: single-task connections, vectored writes, sweeper, admission by declared length, memory watcher, shedding | yes |
| N4 | Ops surface: `/health`, `/ready`, `/metrics`, drain with 1012, `CF-Connecting-IP`, per-IP limits, HTTP errors for unknown paths | no |
| N5 | `relay-loadgen` with histograms and the scenarios above | no |
| D1 | Replace the v0 binary on the VM with the new relay through the same deploy script; VM trims the owner approves; `GOMEMLIMIT` for cloudflared | no |
| N6 | Measurement on a separate host: allocator, QUIC versus HTTP/2, runtime threads, codec rule; record results in the repo | yes, alone |
| X3 | Worker to contract parity, moved into the new repo; hibernation test; 90 s idle | no |
| N7 | Fuzz targets (handshake, frame header, envelope, token), hostile suite, cargo-deny, reproducible release builds | yes |
| X6 | Write tokens (Ed25519, `kid`, expiry, room and role) with Rust and WebCrypto verifiers and vectors | no |
| X7 | `DurableStore`: native log and DO SQLite with a hash-chain head, batched writes | yes |
| C1 | Cut-over: `relay-test` to `relay.fundaments.work`, Worker kept on a fallback name, rollback runbook | no |

### Owner decisions

1. Confirm the production target: the Oracle relay as `relay.fundaments.work`, the
   Worker as fallback (your words suggest yes).
2. Read section 2.8 of the Cloudflare master agreement for non-HTML proxying
   (Cloudflare's service terms restrict large media; small WebSocket frames look
   fine [I]).
3. Fragmented WebSocket messages: reject (1002) or reassemble.
4. `panic = abort` (smaller) or unwind (one bad frame cannot take down 500 users).
5. Write-token transport: query parameter (works from browsers; must never be
   logged) or header.
6. Durable rooms in the first version, with retention and quota.
7. VM trims: remove `containerd`/`runc`, mask `fwupd`, set `Prompt=never` for the
   release upgrader; leave `snapd` (hosts Oracle's agent); `multipathd` only after
   a check. Zram, a small swap file, or neither.
8. Which machine runs the load generator (your desktop over the real tunnel, or a
   second VM over loopback).
9. The Oracle idle-reclaim risk: accept, or move to pay-as-you-go.
10. Frame cap: keep the 8 MiB contract with admission permits.
