# ADR 0013: Live co-editing

Status: accepted and implemented for Beta 1 (text files, one per session), 2026-10-18; stabilised 2026-10-20 (waiting for a host, snapshot sidecar, guards, trailers; see the last implementation note). Project-wide sessions are decided in `docs/adr/0014-project-live.md` (plan: `docs/plans/2026-10-20-live-collab-v2.md`). Evidence: `docs/research/2026-10-18-live-spike.md`. Where the implementation differs from the decisions below, see "Implementation notes".

Status note (2026-10-20): the relay server (the Rust `manang-relay` crate and the Cloudflare Worker in `relay/worker`) now lives in a separate private repository and is no longer part of this repository. The app keeps only the client transport, `crates/manang-relay-client` (`RelayClient`, with the `tls` feature). The wire contract described in this ADR (`/room/<32 hex>`, binary frames, sealed envelopes) remains the public contract that any relay must follow. Paths to `crates/manang-relay` and `relay/worker` below are historical.

## Context

The owner decided that real-time co-editing is required for Beta 1 (invite
only, per-tester private links, Linux / Windows / macOS). The project's stance
is local-first, files as plain UTF-8, git as the source of truth, Rust-first,
no Manang-operated service and no telemetry. Live editing needs a
rendezvous point, so it forces an exception.

## Decision

1. **Engine.** Loro as the CRDT, in the Rust core (`manang-live`), behind the
   `TextCrdt` trait so yrs (already implemented and tested) or another engine
   can replace it with one adapter. The rope stays the buffer of record; a
   bridge converts rope edits (characters or UTF-16) to engine edits (UTF-8
   bytes) and remote updates back to rope changes.
2. **The file stays plain.** A session seeds from the file; autosave writes the
   merged text; the CRDT state is a disposable sidecar in the app data
   directory; "Get updates" is disabled for files in a session; a commit during
   a session lists editors as `Co-authored-by:` trailers; differing starting
   files are resolved by showing a diff (host wins) or a three-way merge
   against the git base, never silently.
3. **Frontend.** No JS CRDT in the web view. The web view sends rope-level
   edits and receives remote change lists and resolved carets; a CodeMirror
   transaction annotated remote, kernel-origin and outside history applies
   them, so undo stays per user.
4. **Transport.** A `SyncTransport` trait (synchronous, poll-based; the core is
   sans-IO and `wasm32`-capable). First implementation: WebSocket to a relay.
   Direct peer connections (iroh) later.
5. **Exception to "no Manang-operated service".** Manang may operate (or the
   user may self-host) a **relay that sees only ciphertext**. It forwards
   opaque binary frames between connections that name the same room id, stores
   nothing, logs no payloads, enforces size and rate limits, and forgets a room
   when it empties. It carries no accounts and no telemetry. A policy key
   (`collab.relays`) lets an organisation restrict or replace it, and
   `collab.live` switches the feature off.
6. **Encryption.** The room secret (32 random bytes) travels in the invite
   URL **fragment**, which is not sent to servers, together with the host's
   seed fingerprint. HKDF-SHA256 derives (a) an AEAD key and (b) a 16-byte
   room id; the relay learns only the id. Each frame is an envelope
   `version(1) | kind | nonce(24, random) | XChaCha20-Poly1305(ciphertext+tag)`
   with associated data `version | kind | room id`. Kinds (document data,
   presence) are visible to the relay for rate limiting. Crates: RustCrypto
   `chacha20poly1305`, `hkdf`, `sha2`, `zeroize`. Properties and non-goals:
   confidentiality and integrity against the relay and outsiders; no
   authentication between members (anyone with the invite can impersonate a
   member); no forward secrecy (rotate by starting a new room); metadata
   (room id, sizes, timing) is visible to the relay.
7. **Presence** (names, carets) is ephemeral: never persisted, expires after
   20 s of silence, ordered by a per-peer sequence number, bounded in size.
8. **Hosting.** Default for the beta: a **Cloudflare Worker routing to one
   Durable Object per room id using the WebSocket Hibernation API**; the
   object holds no document state, only the live sockets, and broadcasts each
   incoming binary frame to the others. Verified from Cloudflare's
   documentation (2026-10-18): Durable Objects (SQLite-backed) are on the Free
   plan (100,000 requests/day, 13,000 GB-s/day); on Paid, 1 M requests and
   400,000 GB-s/month included, then $0.15 per million requests and $12.50 per
   million GB-s; WebSocket messages are billed 20:1 as requests; hibernating
   objects accrue no duration charges; up to 32,768 hibernatable sockets per
   object; WebSocket messages up to 32 MiB. The self-hosted `manang-relay`
   binary (tokio + tokio-tungstenite, no TLS or C code in the default build)
   implements the identical wire contract and remains the answer for
   organisations that forbid third-party cloud services. Nothing is deployed
   by this ADR.
   **Status (2026-10-18):** the Worker is implemented in `relay/worker`
   (Worker + `Room` and `Limiter` Durable Objects, unit tests in workerd, and a
   conformance test that runs the Rust `RelayClient` against `wrangler dev`) but
   **not deployed**; the lead deploys it with `wrangler deploy`. It has no
   backlog, like the Rust relay, and drops the global room and connection caps
   in favour of per-address limits and a 15-minute idle-room timeout.

## Wire contract (both relays)

`GET /room/<32 lowercase hex>` upgrades to a WebSocket. Only binary messages;
the relay checks only length >= 42 and the first two bytes (version 1, kind 1
or 2). Each frame is forwarded to every other socket of the room, never echoed.
Limits (defaults): frame 16 MiB, 200 messages/s and 4 MiB/s per connection,
20 presence messages/s, 16 peers per room, 1,024 rooms, 90 s idle timeout, 25 s
ping. Slow receivers are dropped (they reconnect and resync). `GET /health`
answers `ok`.

## Implementation notes (2026-10-18)

What was built, and where it is not what the decisions above describe.

* **Crates.** `manang-live` (core, still pure and `wasm32`-capable), `manang-relay` (server and a `RelayClient` transport) and the new `manang-live-plugin` (the first-party `live` plugin, registered in `src-tauri`). The plugin is OS-side glue: tokio only moves frames; each session runs on **its own OS thread** that owns the CRDT session, the rope mirror and the transport (not a rayon task: a session is a long-lived loop, and a rayon worker blocked for its lifetime would starve the pool).
* **Edit protocol (refines decision 3).** The web view and the session thread agree on order with a version counter, as CodeMirror's collab module does, with Rust as the authority: local edits are pushed with the version the view has seen and refused (`accepted: false`) if Rust has moved on; the view then integrates the steps it has not seen, rebases its pending edits (`ChangeSet.map`) and pushes again. Remote batches and acknowledgements travel on one ordered event stream (`live.remote`), carets on `live.presence`, connection on `live.state`. No JS CRDT, no transformation in Rust. Convergence is tested with random typing, remote edits and message delays (`binding.test.ts`).
* **The shared copy.** The editor plugin's document for the file is updated step by step by the session thread (`Editor::replace_silent`, no undo entry), so compile, search and the agent read the live text; the web view's `KernelSync` is paused for that document and reconciles on leave.
* **Invite.** `manang-live://join?relay=<ws[s] origin>&file=<project path>#<secret and seed fingerprint>`, not an https link: the key stays in the fragment, the relay address and a file hint are not secret. The relay address comes from the setting `live.relay-url` (empty turns sharing off with "Set a relay in Settings"; joining uses the invite's address). The default is the relay run by Fundaments (`wss://relay.fundaments.work`, a Cloudflare Worker, ciphertext only); the setting can point anywhere or be emptied. (Changed 2026-10-19; it was empty before the hosted relay existed.)
* **Differing copies (refines decision 2).** `plan_join` runs with no git base. An identical copy seeds locally; an empty copy joins directly; anything else asks and, if confirmed, **replaces the copy with the host's version after saving it to `live-trash` in the app config directory** (not `<git-dir>/manang-trash/`: the plugin has no path or repository for a kernel document). The three-way merge option waits for `manang-merge`.
* **Transport.** `wss://` is a cargo feature of `manang-relay` (`tls`: rustls on ring, bundled Mozilla roots; no aws-lc), enabled by the plugin; the relay binary stays TLS-free.
* **Collaborator colours.** Eight slots (the core's `PALETTE_SLOTS`), each a mix of existing tokens plus a stroke style and always a name label; see `docs/design-system-additions.md`.
* **Stabilisation (2026-10-20, wave 0 of the v2 plan).**
  * *Joining without a host.* A join no longer blanks the file or reports Live. The session enters `waiting` (the bar says "Waiting for host", the file is read-only and untouched) and leaves it only when a peer answers, never on a timer. It keeps asking (a Request carrying a one-byte waiting marker, which peers always answer), does not answer others meanwhile, and after 20 s reports "Nobody has answered yet" as an error that clears when someone does. The file is replaced by the room's text only then, in the same ordered `live.remote` step that carries the room's text; the copy is saved to `live-trash` first (and again if it was edited while waiting).
  * *Snapshot sidecar.* Each participant writes a debounced snapshot (`<app data>/live-snapshots/<room id>.snap`: magic, text digest, Loro snapshot; unused ones are pruned after 30 days). A join of the same room uses it only if the file on disk equals the snapshot's text exactly and the digest checks out, so a document survives everyone leaving and the room is reseeded by whoever rejoins first; otherwise the disk wins and the normal join (and its question) applies. A host cannot reopen a room except through its own invite, because `live.start` always makes a new room.
  * *Guards.* Get updates and restoring the live file are refused with a plain message while a session runs; a watcher-driven reload of a live file is not applied (it would be sent to the room as typing): the session's text is kept and the next save overwrites the disk copy.
  * *Trailers.* `live.contributors` returns ready-made `Co-authored-by:` lines (given the committer's identity); "Save a version" passes them to `git.save-version` (new `trailers` argument) and names contributors without an email in the message. People share an email only through the setting `live.email`.
  * *Errors.* A soft error (state `error`) clears on the next state change or remote step. On a digest mismatch at equal versions the peer with the higher id rebuilds its document from the room (`live.resync`, also the bar's Re-sync button), so two peers never wait for each other.
* **Not done.** Deploying the Cloudflare relay (it is built in `relay/worker`, nothing is deployed); policy keys `collab.live` and `collab.relays`; figures; peer-to-peer transport; compaction of long sessions; the three-way merge for differing copies. Project-wide sessions are built and decided in ADR 0014 (what it does not do is listed there); fuzzing of the import path, the protocol decoders and the path rules is done (ADR 0014, "Hardening").

## Consequences

* Positive: the beta ships real-time co-editing with a thin, auditable,
  replaceable server; local files and git are untouched; the core is testable
  without a network (simulated partitions) and runs in a browser later.
* Negative: a new operated service (availability, abuse handling, privacy
  wording, a Cloudflare dependency for the default); Loro's memory grows with
  history (about 9 MiB at two hours of typical editing; mitigated by
  compaction); invitees are trusted equally; new `loro` and RustCrypto
  dependencies; a documented gap between "who edited" (a claim) and
  authentication.
* Follow-ups: chunk the first sync (done for project sessions, ADR 0014), browser entropy and
  TLS client, policy keys, design sign-off for caret colours, work packages L1
  to L9 in the research report.
