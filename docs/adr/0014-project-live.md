# ADR 0014: Project-wide live sessions

Status: accepted and implemented on `feat/live-collab-v2`, 2026-10-20. Extends
ADR 0013 (one text file per session), which stays valid for single-file
invites. Plan and work packages: `docs/plans/2026-10-20-live-collab-v2.md`.
Fuzzing and relay conformance (work package P7) are described under
"Hardening".

Status note (2026-10-20): the relay server (the Rust `manang-relay` crate and the Cloudflare Worker in `relay/worker`) now lives in a separate private repository and is no longer part of this repository. The app keeps only the client transport, `crates/manang-relay-client` (`RelayClient`, with the `tls` feature). The wire contract described in this ADR (`/room/<32 hex>`, binary frames, sealed envelopes) remains the public contract that any relay must follow. Paths to `crates/manang-relay` and `relay/worker` below are historical.

## Context

ADR 0013 shares one text file. People work on projects: a thesis is a main file,
chapters, a bibliography, figures. Sharing the whole folder means that someone
holding an invite now **writes files on other people's disks**, so the design
is as much a security boundary as a sync protocol. The relay, the envelope and
the "no accounts" stance of ADR 0013 do not change.

## Decisions

### 1. Data model

* **One project document, one document per text file.** The project document
  (`manang-live::project`) is a Loro movable tree. Every node is a file or a
  directory with a stable 128-bit `FileId` (`peer << 64 | counter`, so ids
  never collide and need no allocator) and the metadata `{name, kind, size,
  sha256, seed_fp}`. A rename or a move changes the node, not its id. Text
  content is not in the tree: each text file has its own Loro text document,
  loaded lazily.
* **Binary and figure files** are tree nodes plus content-addressed blobs
  (`manang-live::blob`): SHA-256 address, chunks of 1 MiB, at most 64 MiB per
  blob. A text file over 8 MiB is shared as a blob. Blobs are last-writer-wins.
* **Paths are derived, never stored.** `resolve` turns the tree into unique
  paths the same way on every replica. A remote name that is not a valid file
  name becomes `invalid-<id>`. Two directories with the same case-folded name
  under one parent merge. Two other nodes with one path collide: the one
  created first keeps the name, the others are shown as `stem (conflict
  xxxxxxxx).ext`. Limits: names 255 bytes, paths 1,024 bytes, depth 64, 65,536
  nodes.
* **Manifest.** A list of `(id, path, kind, size, sha256, seed_fp)` in pages of
  at most 256 entries (65,536 entries, 256 pages in all). Its *root* is a
  16-byte SHA-256 prefix over the content fields only (path, kind, size, hash,
  seed fingerprint; not ids, not order), so a joiner can compute the root of its
  own folder and the invite carries the host's. `ManifestAssembler` accepts
  pages in any order, validates every entry and checks the root before
  returning the list.

### 2. Wire v2 and the room id

The envelope (version 1, kinds Data and Presence, XChaCha20-Poly1305) is
untouched, so neither relay changes. New `Message` variants are appended to
the postcard enum (the old indices never move): `DocSync`, `DocUpdate`,
`Manifest`, `BlobGet`, `BlobChunk`, `ManifestGet`. `DocId` names the project
tree or one file. Decoding enforces bounds (version vectors 64 KiB, manifest
pages and entries, chunk indices, chunk size). A project room derives its
routing id with HKDF info `room-id-v2` from the same secret and the same AEAD
key (`RoomSecret::derive_project`), so a project room and a file room never
share a relay room and an old client never receives v2 traffic. The v2
invite is `manang-live://join?v=2&scope=project&relay=...#<secret><manifest
root>`; single-file invites parse as before.

### 3. Join: the plan and the resolve rules

Joining has two phases and changes nothing on disk in the first.

1. The guest connects and asks for the manifest (phase `manifest`). When it
   arrives, the guest's folder is classified against it, through the path
   policy, and `live.plan` lists each file as `identical` (same SHA-256),
   `missing` (the host has it, the guest does not), `differing` or `guest-only`.
   The session waits in phase `plan`.
2. `live.resolve` supplies the choices. `identical` and `missing` files are
   taken without a choice. A `differing` file needs `take-host` (replace) or
   `keep-mine` (it stays out of the session on this side). A `guest-only` file
   joins only with an explicit `share`; the default is `keep-private`.
   The wildcard path `"*"` is the one all-files confirmation: its action
   covers every differing file without an explicit choice (explicit choices
   win). A differing file with no choice and no wildcard is refused
   (`unresolved`); **more than one replacement needs `"*"` with `take-host`**,
   even when each file was chosen by hand (`needs-confirmation`). `share` on a
   differing file is invalid. No three-way merge: the host's version wins, and
   the guest's copy is kept (below).

### 4. Disk rules and the path deny-list

Every path from the network passes `pathsec` before anything touches the disk,
and again when a debounced write runs.

* **Lexical rules**: not empty, at most 1,024 bytes and 64 components, no `\`,
  not absolute, no drive or UNC prefix, no `.` or `..` or empty component,
  valid file-name characters (no control characters or `<>:"|?*`, no trailing
  dot or space), no Windows device names (`CON`, `NUL`, `COM1`...).
* **Deny-list**: any component starting with `.` (so no dotfiles, `.git`,
  `.manang`), `manang.toml` at any depth, and the directories `target`,
  `node_modules` and `__pycache__` at any depth, all case-insensitively.
  Paths the project's `.gitignore` ignores are not shared in either direction.
* **File system rules**: a symbolic link as the last component is refused and
  never followed or shared; the deepest existing ancestor is canonicalised and
  must stay inside the project root; a name that differs only by case from an
  existing sibling is refused when the policy is case-insensitive (on by
  default on Windows and macOS; see "Not done").
* **Sizes**: 8 MiB for a document, 64 MiB for a blob.
* Writes are atomic. Text writes are debounced (500 ms of quiet, last text
  wins). A file open in a tab is written by the web view's autosave only.
  Echoes of the session's own writes are recognised for 10 s and not shared
  again.
* A local change that is a large rewrite (a `git checkout`: more than 25 files
  in a batch, or more than 50 % of the lines of the changed text files when at
  least 40 lines are involved) is held, not shared, until the person chooses
  to share or to discard it (`live.paused`, `live.resync`).

### 5. Trash and backup behaviour

Nothing is deleted outright. A deleted file and a file replaced by a remote
version go to the project's recoverable trash `.manang/trash/<id>/<path>`. A
`take-host` replacement copies the guest's file there when the host's content
arrives, not before. A debounced write that would overwrite content someone
else put on disk since the session last wrote the file first copies that content
to the trash. Leaving a session never deletes files. A remote delete of a file
open in a tab keeps the tab as "deleted by X"; `live.restore` recreates it.
(Single-file joins keep using `live-trash` in the settings folder, as in ADR
0013.)

### 6. Sidecar and eviction

A session holds at most 32 text documents in memory (files open in a tab, or
whose latest updates have not been confirmed, are pinned and never evicted).
The least recently used are parked as **full** snapshots in
`<app data>/manang-live/<room>/<file id>.loro`. A sidecar records the SHA-256 of the
file on disk when it was written and is used only if the disk still has exactly
that content; its body carries its own hash and length; any damage makes it a
miss and the file is seeded from disk again. Sidecars are pruned after 30 days.
The sidecar is a cache: deleting it loses nothing the room or the disk lacks.
A parked document misses live updates; it catches up when the editing peer's
size and hash update arrives (about 1.5 s after typing pauses) or when it is
loaded again.

### 7. Pacing

A project's first sync is hundreds of messages. The relays cap one connection
at 200 messages/s, 4 MiB/s and a 16 MiB frame, and Cloudflare bills Worker
WebSocket messages 20:1 (see "Consequences"). `manang-live::pacer` sits
between the session and the transport: a token bucket of **2 MiB/s** (burst 512
KiB) and one of **100 messages/s** (burst 20), four classes (interactive, priority,
normal, bulk) served in order. Interactive frames up to 16 KiB bypass the queue
(and are charged to the buckets), so typing never waits behind a blob. Blob
chunks are 1 MiB, so no frame exceeds about 1 MiB. Progress events are throttled to
about ten per second. The pacer sends at half the relay's byte limit and half its
message limit.

### 8. Presence v2

`PresenceMessage::StateV2` adds the file the person is editing (`doc`) and the
files they have open (at most 16, no repeats) to the v1 state (name, colour slot,
email if the person typed one, selection in `doc`). It is clamped on decode
as v1 is (names 64 bytes, selections 256 bytes, email shape). It is ephemeral,
expires after 20 s, and travels in Presence envelopes. Following someone is local
state and sends nothing.

### 9. Host-leave warning

`live.status.unsynced_files` (and the same count in `live.peers`) is the number
of files that no *present* peer has shown it holds: a peer holds a text file if it
answered for it, was sent its state, confirmed it or edited it, a blob if it was
served to it. The UI says "N files not yet synced to others" before the host
leaves. Leaving is never blocked.

### 10. What is NOT done

* **Identity and roles.** One shared secret; a "read-only guest" would be a
  courtesy of the interface only, so none is offered. Needs per-member keys and
  a signed membership log.
* **Revocation.** There is no way to remove one person; end the session and start
  a new one (a new secret).
* **Comments and suggestions.**
* **A durable room.** The relay stores nothing. A room whose members all left is
  gone; joining an empty room is not possible and the UI says so. A host
  keeps a local sidecar, not a room.
* **Version history inside a session** (named versions, checkout).
* **The figure CRDT.** Figures and images are blobs, last writer wins; concurrent
  edits of one figure lose one side.
* **Peer-to-peer transport.**
* **Case-folding on Windows and macOS.** The collision logic is unit-tested by
  forcing it on, on Linux. It has not been run on a case-insensitive file system,
  nor Windows reserved-name or long-path behaviour on a real Windows volume.
* **Merging differing copies** (waits for `manang-merge`).
* **A hostile-safe yrs integrator** (see Hardening); only Loro faces peers.

## Threat model

Trusted: nobody beyond what the invite already grants. Assume the relay and the
network are hostile and any holder of the invite may be malicious.

* **An invite holder can write to guests' disks**, within the allow-list above:
  create, change, rename and delete shareable files inside the project folder,
  and nothing else (no paths outside it, no dotfiles, no `.git`, no `manang.toml`, no
  symbolic-link escape, no oversize files). They can overwrite or remove a
  guest's own files, which is why every replacement goes to the trash first and a
  join that replaces more than one file needs the explicit all-files
  confirmation. They can plant files the guest later opens or compiles, so the
  invite is a capability to be sent only to people the guest would let edit the
  project.
* **No member authentication.** Names are claims. Anyone with the secret can
  act as any member.
* **The relay** sees ciphertext, the envelope kind, the room id (derived, not
  reversible to the key), frame sizes, timing and connection addresses. It cannot
  read or forge frames (AEAD), but it can drop, delay, duplicate or replay them;
  the sync protocol tolerates all four.
* **Hostile bytes** reach `Message::decode`, `PresenceMessage::decode`,
  the manifest and blob assemblers, `Envelope::open`, and the CRDT imports of
  Loro and yrs (project tree and documents). These and the path rules are
  fuzzed (see below).
* Out of scope: a malicious local user, the guest's other programs, and a
  compromised machine.

## Hardening (work package P7)

Fuzz targets in `fuzz/` (one per attack surface, see `fuzz/Cargo.toml`):
`live_message_decode`, `live_presence_decode`, `live_manifest` (paginate and
assembler round trip, hostile pages), `live_blob`, `live_envelope_open`
(random keys, nonces and frames, tampering, cross-room replay), `live_text_apply`
(Loro and yrs `apply_update` and `from_snapshot`), `live_project_apply`
(`ProjectDoc`), `live_pathsec` (an accepted path never contains `..`, is
relative, avoids the deny-list and stays inside the root when joined),
`live_fsbridge` (`parse_changed_event`; `apply_edits(old, text_edits(old, new))
== new`). The CRDT targets repair Loro's header checksum so mutated bodies reach
the decoder, and mutate genuine updates and snapshots rather than only random
bytes. Run one with `cargo +nightly fuzz run <target> -- -max_total_time=60`.

Findings, all fixed or recorded (seeds in `fuzz/regressions/live_text_apply/`):

* **Loro cursor (fixed).** Resolving a peer's presence cursor that names a map or
  tree container reached an `unreachable!()` inside Loro, a remote crash of the
  session thread. `LoroEngine::cursor_pos` now resolves only its own text
  container.
* **yrs decoders (fixed) and yrs integration (not fixed).** A client id of 2^53 or
  more panics yrs while decoding; `YrsEngine` now decodes behind a guard. yrs can
  also panic inside a transaction on a crafted block, and its transaction's drop
  then aborts the process, which no guard can stop. The session uses Loro, not
  yrs (yrs remains the benchmark alternative behind the `TextCrdt` trait), so
  this is a documented limit: do not put `YrsEngine` behind a network without
  fixing that path. The fuzz target feeds hostile updates to Loro only.
* The fuzz invariant for the project tree is that same-named directories may
  share a path (they merge) and every other node has its own.

The relay test `crates/manang-relay/tests/bulk_sync.rs` replays the pacer's output
for a 100-file join (170 messages, 4.9 MiB) through the Rust relay with its default
limits and asserts that no peer is dropped, order is kept, and no second of the
traffic exceeds 200 messages or 4 MiB (measured on the run of 2026-10-20: 170
frames, 4.89 MiB in 3.3 s, at most 118 messages and 2.97 MiB in any one second). The same test runs against the Cloudflare
Worker under `wrangler dev` when `MANANG_RELAY_URL` names its `ws://` address
(see `relay/worker/README.md`; `pnpm conformance` runs it). Not run against the
Worker as part of P7.

## Consequences

* A project room is one relay connection per person, so cost scales with
  messages, not files. **Cloudflare bills Worker WebSocket messages 20:1 as
  requests** (ADR 0013, from Cloudflare's documentation of 2026-10-18; check it
  again before relying on a figure). The test's join workload is 170 messages
  from the host (1 manifest page, 140 small document frames, 25 one-chunk blobs, 4
  chunks of two larger blobs) and the guest answers with its own requests (about
  one per file and per chunk; an estimate, the test does not measure it), say
  280 in all: 280 / 20 = 14 billed requests per 100-file join. The Free plan's
  100,000 requests a day is about 7,000 such joins a day, and Paid's included
  1 million a month about 70,000 joins; extra requests cost $0.15 per million, which
  is about $0.000002 per join. Pacing does not change the message count, it
  bounds the rate.
* A connection's byte bucket holds two seconds of its rate (8 MiB), so the
  largest frame one connection can send is 8 MiB, not the 16 MiB frame limit
  (the test `frame_limits_as_enforced` pins this). The project wire never sends
  more than about 1 MiB in a frame; a first sync of one large single-file document
  is a single frame and is subject to the 8 MiB bound.
* Hosts keep Loro documents for up to 32 text files in memory and park the rest;
  the idle-memory budget is a function of that cap.
* Memory and disk: sidecars are full snapshots, so history grows until pruned
  after 30 days.

## Follow-ups

Member identity and roles; epoch-key revocation and expiring links; anchored
comments; an optional durable room; version history and shallow-snapshot epochs;
figures as a CRDT; a verification pass of the case-folding rules on Windows and
macOS; a second fuzz round with the structure-aware corpus kept in CI (`full` tier).
