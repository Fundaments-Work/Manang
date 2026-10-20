# Live co-editing spike (collaboration WP5)

> Note (2026-10-20): The relay server moved to a separate private repository on 2026-10-20 (`crates/manang-relay` and `relay/worker` no longer exist here; the app keeps `crates/manang-relay-client`). Paths below describe the state when this was written.

Date: 2026-10-18. Branch `agent/live-spike`. Code: `crates/manang-live`,
`crates/manang-relay`, `src/lib/editor/live/`. Decision record:
`docs/adr/0013-live-editing.md`.

## Verdict

**Ship it for Beta 1. Stack: Loro (CRDT) in the Rust core, behind the
`TextCrdt` trait; the rope stays the buffer; an in-house CodeMirror binding
over Tauri IPC (no JS CRDT in the web view); XChaCha20-Poly1305 end-to-end
encryption with the room secret in the invite fragment; a WebSocket relay that
sees only ciphertext, first as a Cloudflare Worker + Durable Object (the Rust
`manang-relay` binary is the self-hosted fallback and the local test server).**

Everything the plan asked of the first milestone works in the spike: two
clients, one relay, encrypted updates, remote cursors (core + view prototype),
convergence under reordering / duplication / loss / partitions, and a keystroke
bridge at roughly 10 microseconds against an 8 ms budget. What is left is
product work (IPC commands, UI, Cloudflare Worker, policy), not research.

## Measurements

Release build, this machine, 1 MB / 200 KB documents of mixed ASCII, accented,
CJK and emoji text. `cargo run --release -p manang-live --example measure`
(counting allocator, so heap figures are live bytes) and `cargo bench -p
manang-live` (criterion). Both engines sit behind the same trait and the same
bridge, session and tests.

| Measurement | Loro 1.16.2 | yrs 0.28.0 |
| --- | --- | --- |
| (a) 10k random local edits, 200 KB doc | 106 ms (10.6 us/edit) | 1,103 ms (110 us/edit) |
| (a) engine heap: after seed -> after edits | 293 KiB -> 4.4 MiB | 202 KiB -> 4.2 MiB |
| (b) apply the 10k edits as remote updates | 83 ms (8.2 us/update) | 1,768 ms (161 us/update) |
| (c) state after seed (text 200 KiB) | 162 KiB | 200 KiB |
| (c) full state after 10k edits | 394 KiB | 386 KiB |
| (c) trimmed (shallow) snapshot | 204 KiB | 386 KiB (no trimming exists; GC is already on) |
| (c) incremental update per edit (mean / max) | 100 B / 134 B | 18 B / 32 B |
| (c) heap after reload from full / trimmed snapshot | 2.3 MiB / 2.1 MiB | 4.0 MiB / 4.0 MiB |
| (d) local keystroke through the bridge, 1 MB doc: p50 / p99 / p99.9 | 9.5 / 43 / 302 us | 12.6 / 57 / 126 us |
| (d) same + AEAD seal: p50 / p99 | 11.7 / 46 us | 14.6 / 59 us |
| (d) remote update: import + rope patch: p50 / p99 | 10.0 / 27 us | 34 / 70 us |
| (d) criterion mean, local keystroke / + seal / remote | 6.4 / 8.7 / 9.7 us | 87 / 125 / 66 us (degrades as edits accumulate) |
| (g) 2 h session, 57.6k edits, 3 peers, authors alternate every edit: heap at 2 h | 45 MiB (worst case) | 2.9 MiB |
| (g) same, author changes every 40 edits (typical): heap at 2 h | 9.2 MiB | 2.9 MiB |
| (g) full state at 2 h (doc ends 86 KiB) | 451 KiB (110 KiB trimmed) | 226 KiB |
| (g) heap after compaction (reload from trimmed) | 3.6 MiB | 2.9 MiB |

Notes on reading the table:

* **The keystroke budget is met by a factor of ~200 with either engine** (the
  whole path must stay under 8 ms; the bridge is 0.01 to 0.1 ms, max observed
  4.4 ms once, a Loro allocation spike). The choice is therefore not about the
  budget.
* **yrs needed byte offsets to be competitive.** In its UTF-16 mode every edit
  scanned the whole document (about 260 us per edit on 1 MB, measured in
  isolation). The engine trait therefore takes UTF-8 byte offsets, which the
  rope converts to and from in O(log n); yrs is then within 2x of Loro on the
  keystroke path. It still walks its block list for every lookup, so random
  edits cost 10x Loro, and remote import 20x on bulk. That matters for paste,
  find-and-replace, reformatting and the first sync of a big file, not typing.
* **Loro's cost is memory growth**, not speed: its change store keeps per-op
  metadata (about 800 B per alternating-author edit resident; 9 MiB at two
  hours of realistic typing, 45 MiB in the adversarial pattern). The idle
  budget is 75 MB, so this needs mitigation (below), but it is bounded by
  session length and resets on rejoin. `compact_change_store` only recovers
  about 3%; reloading from a shallow snapshot recovers 90%.
* Both engines were **equal on convergence**: 200 proptest cases per property
  for each (hostile network; sequential oracle; unicode mapping, 300 cases).
  The proptest run found one real engine pitfall (below).
* Automerge was not measured (the plan already rated it a weaker fit; no time
  was spent on it).

### Decision table

| | Loro | yrs |
| --- | --- | --- |
| Speed, bulk and random edits | much faster | slower (block-list lookups) |
| Typing latency | ~10 us | ~13 us |
| Memory over a long session | grows (9 to 45 MiB at 2 h); fixable by rejoin or shallow snapshot | tiny and flat |
| Update size per keystroke | ~100 B | ~18 B |
| History trimming | yes (`ExportMode::shallow_snapshot`) | none needed, none possible |
| Stable cursors | built in (`Cursor`) | built in (`StickyIndex`) |
| Versions equal => content equal? | yes (version vector covers deletes) | **no**: a state vector omits deletions; we add a delete-set digest to the version |
| Panic risk on bad offsets | returns an error | **panics** on an offset inside a character (seen in a test); the bridge always passes boundaries, but a malformed remote update is the residual risk |
| Figures (movable tree/list/map for the scene) | yes, same document | maps and arrays, no movable ops |
| Browser story | wasm build officially maintained | wasm via yrs-wasm / Yjs itself |
| Licences, C code | MIT; pure Rust (`psm` asm shim already in the app tree) | MIT; pure Rust |

Loro wins on speed, history control, the figure scene and explicit errors;
yrs wins on memory. Because the engine is behind a trait, **switching later
costs one adapter**, which is the real mitigation for the young-engine risk.

## Rope bridge

`manang_live::bridge::LiveDoc<E>` over any `TextTarget` (implemented for
`ropey::Rope`). Local edits arrive as character ranges (the editor plugin's
unit) or UTF-16 ranges (CodeMirror's); remote updates come back as
`AppliedChange { char_start, char_delete, utf16_start, utf16_delete, insert }`
so Rust applies the char form and the web view the UTF-16 form unchanged.
Properties tested with proptest on text full of emoji, ZWJ families, regional
indicators, combining marks, CJK, `\r\n` and U+10FFFF: rope == CRDT == a
`Vec<u16>` patched only from the change lists, after every edit, for both
engines; offsets inside a character or surrogate pair are refused (an error,
never corruption); the loro engine reports Unicode scalars, yrs bytes, and the
bridge normalises both. The p99 for one local keystroke on a 1 MB document is
**43 us (Loro)**.

## Convergence and sessions

`manang_live::sim` is a deterministic virtual-time network (random delay =>
reordering, duplication, 10% loss, partitions). Property tests: all clients
byte-equal after healing (and equal to the CRDT text), and equal to a plain
`String` oracle for sequential edits. Protocol: `Request / Reply / Final`
state exchange on every (re)connect plus a 10 s anti-entropy request (skipped
when versions are equal), pending updates trigger an immediate request, no
outbound queue (the CRDT is the queue). Safety net: sync messages carry a text
digest; **equal versions with different digests raise `Output::Diverged`**.

Findings worth keeping:

1. **A Yjs state vector cannot see a missed deletion.** Our first yrs run
   failed to converge in the proptest. Fixed by making `version()` include a
   hash of the delete set.
2. **Deterministic seeding is dangerous if fingerprints differ.** Two peers
   that seed different files with the shared seed peer collide on operation
   ids and silently diverge (Loro showed `"host text\nt\n"` against
   `"joiner text\n"` with equal versions). Hence the join plan
   (`seed::plan_join`) and the digest check; a test pins both.
3. yrs seeding in one block is O(file) per edit; irrelevant after switching to
   byte offsets.

## End-to-end (no UI)

`crates/manang-relay/tests/e2e.rs`: a real relay on a random localhost port;
host and guest in an encrypted room; the guest receives the seed; both type
concurrently; the guest's link drops, both keep typing, the guest reconnects on
its own backoff timer; texts equal. The relay's frame tap captured 16 frames
(1.7 KB Loro, 1.2 KB yrs); none contains any of ten plaintext fragments and
all start with the version and kind bytes. `tests/limits.rs` covers health,
bad paths, routing isolation, text/garbage/short/oversized frames, rate
limiting, room and peer caps, idle timeout and room forgetting.

## Encryption design (summary; full text in the ADR)

Room secret: 32 random bytes in the invite URL fragment (never sent to a
server), with the host's seed fingerprint. HKDF-SHA256 (salt
`manang-live/v1`) derives the AEAD key and a 16-byte room id; the relay sees
only the room id, a one-way function of the secret. Envelope v1 =
`version | kind | 24-byte random nonce | ciphertext+tag`; AAD binds version,
kind and room id. Single-bit-flip test over every bit of a frame, wrong room,
wrong key, truncation, appended bytes and a pinned known answer. Crates:
`chacha20poly1305` 0.11.0, `hkdf` 0.13.0, `sha2` 0.11.1, `zeroize` 1,
`getrandom` 0.4 (native only; the core takes an `Entropy` trait so browsers
inject `crypto.getRandomValues`), all MIT or Apache-2.0, versions verified on
crates.io 2026-10-18.

Honest limits: confidentiality and integrity against the relay and outsiders;
**not** authentication between members (anyone with the invite can speak as
anyone, so a presence name is a claim); the relay still sees traffic metadata
(room id, sizes, timing, frame kind); no forward secrecy (a leaked invite
decrypts captured traffic; rotate by starting a new room).

## Relay hosting: Cloudflare vs self-hosted

Facts from developers.cloudflare.com (2026-10-18):

* Durable Objects are available on the Workers **Free** plan (SQLite-backed
  only): 100,000 requests/day and 13,000 GB-s/day, daily reset 00:00 UTC;
  exceeding fails further operations. Paid: 1 M requests/month included then
  $0.15/M; 400,000 GB-s/month included then $12.50/M GB-s; $5/month minimum
  for the Workers Paid plan.
* **WebSocket messages count 20:1** toward requests; duration is billed at 128
  MB wall-clock only while the object is active. The **Hibernation WebSocket
  API** (`ctx.acceptWebSocket`, `webSocketMessage()`) keeps clients connected
  while the object is evicted; idle, hibernatable objects accrue no duration.
  Auto-response ping/pong does not wake the object. Incoming events, alarms,
  `setTimeout`/`setInterval` prevent hibernation.
* Up to 32,768 hibernatable WebSockets per object; WebSocket message limit
  raised to 32 MiB (Oct 2025). Pending outbound I/O keeps an object in memory
  up to 15 minutes. Cloudflare's own example: 100 objects x 100 hibernating
  sockets at 1 msg/min = about $20.65/month.

Sizing a beta session (2 people, 5 messages/s each for two hours = 72,000
frames) = 3,600 billed requests plus a few hundred heartbeat wake-ups; the
free tier's 100,000 requests/day covers about 25 such sessions a day, and a
paid plan ($5) covers thousands. Our frames are <200 B typical, with the first
sync of a large document the only large frame (cap 16 MiB, under the 32 MiB
platform limit).

| | Worker + Durable Object (hibernation) | `manang-relay` self-hosted |
| --- | --- | --- |
| Cost | free tier for beta; $5/mo plus cents beyond | a VPS (about $5/mo) or a home box |
| Ops | none; global edge; TLS and DDoS handled | TLS proxy, patching, monitoring, restarts |
| Latency | nearest edge; the room's object is pinned to one location | one region |
| Trust | one more vendor sees metadata (ciphertext only) | nothing but your own host |
| Isolation | one object per room id; code is ~80 lines of JS/TS | a binary, unit-tested here |
| Enterprise / policy | `collab.relays` allow-list lets a company point at its own relay | the answer for "no third-party cloud" customers |

**Recommendation:** Beta runs the Cloudflare Worker + hibernating Durable
Object (zero ops, free tier, TLS included); keep `manang-relay` as the
documented self-hosted option and the local test server. Both implement the
same wire contract (`/room/<32 hex>`, binary frames, forward to everyone
else), so the client does not know which it talks to. Nothing was deployed.
The ADR records the exception to "no Manang-operated service".

## Frontend feasibility

`src/lib/editor/live/remote.ts` (+ 15 vitest tests, lint and typecheck clean,
not wired in):

* `composeRemoteChanges`: sequential remote changes (each relative to the
  document after the previous) composed into **one** `ChangeSet`;
  randomised test against a string oracle; throws `RangeError` on divergence
  (caller resyncs).
* `remoteTransaction`: one transaction annotated `fromKernel` (the existing
  echo guard; asserted to yield zero kernel ops), `Transaction.remote`, and
  `addToHistory: false`. Test: **local undo and redo touch only local edits**,
  across interleaved remote edits (CodeMirror's history maps its stack over
  remote changes). Note: edits typed within CodeMirror's group delay are one
  undo step; that is existing behaviour.
* The local selection keeps its text when remote edits land before it.
* Remote carets: a state field of `RemoteSelection` (UTF-16, resolved by the
  core from stable CRDT positions), mapped through local transactions
  (`assoc = -1`, so typing at someone's caret does not drag it) until the next
  authoritative update; widget caret (1 px, square) + range tint, all from
  existing tokens via `color-mix` (text, sage, background), no new hex, no
  radius, no shadow. **Open design question for the owner:** the design system
  has two hues and lime is reserved for the single action, so colour alone
  cannot name 8 people; the prototype varies stroke style (solid, dashed,
  dotted, double) and always prints the name.
* Integration note: Rust applies remote changes to the rope *and* emits them
  as an event; the existing `kernelSync` revision bookkeeping must accept
  "revision advanced by N because of a remote batch" instead of treating it as
  a foreign change that forces a resync.

### Binding choice

| | `loro-codemirror` 0.3.3 | `y-codemirror.next` 0.3.6 | In-house over Rust core |
| --- | --- | --- | --- |
| Licence | MIT | MIT | ours |
| Last release | 2025-10-07 (12 months old) | 2026-08-18 (releases rare: 2024-06 to 2026-08) | n/a |
| Runs in web view | needs `loro-crdt` 1.16.4 (20 MB unpacked, wasm) | needs `yjs` + `lib0` | nothing |
| Who owns the CRDT | the web view (duplicate of Rust) | the web view | Rust |
| Fits "frontend is a thin shell" | no | no | yes |
| Undo | bound to its own UndoManager | y-undo | stock CodeMirror history (tested above) |

**Recommendation: in-house binding.** Putting a second CRDT in the web view
would duplicate the document in two engines, force a Rust-to-JS sync layer on
top of the network layer, break zero-copy IPC and Rust-first, and tie us to
bindings that are 12 months stale (Loro) or rarely released (yjs). The
in-house part is about 150 lines and is already prototyped and tested.

## Git interplay (decisions; data and tests in `vcs.rs`, `seed.rs`)

1. The file on disk is the truth: a session **seeds** from the current file;
   autosave writes the merged text to the same file; git sees ordinary UTF-8.
2. The CRDT state is a **disposable sidecar** in the app data directory
   (`<data>/live/<room id>.crdt`), never in the working tree, never committed.
3. A commit during a live session **commits the file on disk** (flush autosave
   first) and lists everyone who edited since the previous commit as
   `Co-authored-by:` trailers (`ContributionLog`, `coauthor_trailers`: the
   committer excluded, same email deduplicated case-insensitively, stable order,
   names and emails sanitised against trailer injection). Peers share a commit
   email only by opt-in; people without one are listed for the committer to
   name in the body. Attribution is per update frame; exact per-operation
   attribution from CRDT peer ids is an upgrade, not a blocker.
4. **"Get updates" is disabled for files in a live session**
   (`live_paths_touched_by`); the message tells the user to leave the session.
5. **Different starting files** (`seed::plan_join`, fingerprint in the invite):
   equal => seed locally with identical operations (nothing transferred); equal
   to the git base (no local edits) => adopt the host's content; otherwise join
   into a shadow buffer, show the diff, and offer *take the host's version*
   (default, the joiner's file copied to `manang-trash` first) or *merge* via
   `merge3` against the git base (WP2), the merged text entering the room as an
   ordinary local edit. Never silent, never blocks the host.

## Risks

* Loro memory growth over long sessions (mitigation: periodic compaction on
  rejoin / an agreed shallow snapshot; the idle budget needs a real-app
  measurement). Engine youth: mitigated by the trait.
* Relay is a service we run: availability, abuse (rate limits and caps are
  in), and legal/privacy wording; metadata is visible to it.
* No member authentication; a malicious invitee can corrupt the document
  (CRDT history and git history are the recovery).
* First sync of a very large document is one frame (cap 16 MiB; chunking is a
  work package); yrs/Loro panics or pathological inputs from a hostile member:
  fuzz the import path (WP below) and consider an isolating thread.
* `wasm32` check passes, but a browser build needs `getrandom`'s wasm
  feature wiring that Loro brings and an `Entropy` from the browser; not run.
* Undo semantics across disconnect/reconnect and grouping are CodeMirror's;
  needs user testing. Edits that split a surrogate pair are refused by the
  bridge; CodeMirror does not normally produce them.
* Rate and size limits and heartbeat intervals are guesses until real sessions.
* Colour/identity design question above.

## Work-package plan (disjoint files)

| WP | Contents | Files | Depends on |
| --- | --- | --- | --- |
| L1 `manang-buffer` hooks | `DocumentBuffer` rope accessor and a history-bypassing `apply_remote`; implement `TextTarget` for it | `crates/manang-buffer/src/*` | none |
| L2 `manang-live` plugin | register `manang-live` as a plugin; commands `live.host {path} -> {invite, room}`, `live.join {invite, path} -> {plan}`, `live.leave`, `live.status -> {state, peers}`, `live.resolve-seed {choice}`; events `live.remote {doc, changes[], revision}`, `live.presence {selections[]}`, `live.state`. Rust thread owns CRDT and session; `RelayClient` on a Tokio handle (I/O only) | `crates/manang-live-plugin` (new) or `src/plugin.rs` in `manang-live` behind a feature, `src-tauri/src/lib.rs` registration | L1, policy seam |
| L3 relay on Cloudflare | Worker + hibernating DO implementing the wire contract; wrangler config; load test against the same `tests/limits.rs` behaviours; deploy only on the owner's go | `infra/relay-worker/*` | none |
| L4 editor integration | wire `remote.ts` into `view.ts`/`documents.svelte.ts`; extend `kernelSync` for remote revision bumps; selection reporting to Rust; remote caret theme tokens; presence strip in the status bar | `src/lib/editor/live/*`, `src/lib/editor/kernelSync.ts`, `src/lib/state/documents.svelte.ts` | L2, design sign-off |
| L5 UI | Share / Join dialogs (invite link with fragment, seed diff view, resolution choice), session banner, "leave session"; vocabulary stays Files/Search/.../Settings | `src/lib/views/live/*` | L2, L4, WP2 (`merge3`) for the merge choice |
| L6 git rules | `git.commit` accepts trailers from `ContributionLog`; `git.sync` consults `live_paths_touched_by`; flush autosave before commit | `crates/manang-git/*` (WP1/WP3 files) | L2, WP1 |
| L7 hardening | fuzz targets for `Message::decode`, `Envelope::open`, `apply_update` (60 s cap); chunked first sync; periodic shallow-snapshot compaction; relay-side metrics endpoint; reconnect jitter; TLS client (`rustls` with ring, as the app does) for `wss://`; browser `Entropy` | `crates/manang-live/fuzz`, `crates/manang-relay/*` | L2 |
| L8 policy | `collab.live`, `collab.relays` keys; relay URL setting; kill switch | settings plugin | policy layer |
| L9 figures | Loro map keyed by node id plus ordered id list for the scene (trait extension or a second document type) | `crates/manang-figure`, `manang-live` | L2 |

Tests per WP: kernel-driven integration tests (L2), the existing proptest
suites kept in CI (they run in about a second), vitest for L4, relay tests
against both relay implementations, a verify-ui run for L5, and a two-instance
manual session before each beta build.

## What was built

`manang-live`: engine trait + Loro + yrs, rope bridge, envelope, invite,
protocol, presence, sans-IO session, `SyncTransport`, simulated network,
git-interplay data, seed plan. `manang-relay`: server + `RelayClient`. Gates:
`cargo fmt`, `cargo clippy -p manang-live -p manang-relay --all-targets
--locked -D warnings`, `cargo test -p manang-live -p manang-relay --locked`
(57 tests including 6 proptest properties), wasm32 `cargo check` of
`manang-live`, vitest + eslint + prettier + svelte-check for the TS files.
