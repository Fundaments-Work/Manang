# ADR 0013: Live co-editing

Status: accepted and implemented for Beta 1 (text files, one per session), 2026-10-18. Evidence: `docs/research/2026-10-18-live-spike.md`. Where the implementation differs from the decisions below, see "Implementation notes".

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
* **Invite.** `manang-live://join?relay=<ws[s] origin>&file=<project path>#<secret and seed fingerprint>`, not an https link: the key stays in the fragment, the relay address and a file hint are not secret. The relay address comes from the setting `live.relay-url` (empty: sharing says "Set a relay in Settings"; joining uses the invite's address). There is no hosted default.
* **Differing copies (refines decision 2).** `plan_join` runs with no git base. An identical copy seeds locally; an empty copy joins directly; anything else asks and, if confirmed, **replaces the copy with the host's version after saving it to `live-trash` in the app config directory** (not `<git-dir>/manang-trash/`: the plugin has no path or repository for a kernel document). The three-way merge option waits for `manang-merge`.
* **Transport.** `wss://` is a cargo feature of `manang-relay` (`tls`: rustls on ring, bundled Mozilla roots; no aws-lc), enabled by the plugin; the relay binary stays TLS-free.
* **Collaborator colours.** Eight slots (the core's `PALETTE_SLOTS`), each a mix of existing tokens plus a stroke style and always a name label; see `docs/design-system-additions.md`.
* **Not done.** The Cloudflare relay (nothing is deployed); policy keys `collab.live` and `collab.relays`; disabling Get updates for a live file; `Co-authored-by:` trailers on commits (the `live.contributors` command exists, nothing calls it yet); figures; peer-to-peer transport; compaction of long sessions; fuzzing the import path.

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
* Follow-ups: fuzz the import path, chunk the first sync, browser entropy and
  TLS client, policy keys, design sign-off for caret colours, work packages L1
  to L9 in the research report.
