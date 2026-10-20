# Live collaboration v2: from one file to a project

Date: 2026-10-20. Builds on ADR 0013 and `2026-10-18-collaboration.md`. Inputs:
a read-only code audit, a state-of-the-art survey and a project-session design,
all done on 2026-10-20 by agents. The audit's claims were spot-checked
(no live guard in `versionActions.ts`, `live.contributors` has no UI caller,
`sync_timeout_ms` of 3 s moves a session to Live without a host). Survey claims
about Loro's API (movable tree, cursors, marks, shallow snapshots) come partly
from background knowledge and **must be checked against docs.rs/loro before
use**; the loro.dev docs were unreachable during research.

## Where we are

One text file per session, equal trust, ciphertext-only relay, caret presence.
Core sync is sound (sans-IO session, anti-entropy, offline merge, per-user undo).
It feels small because everything around the document is missing.

## Wave 0: stabilise what exists (no owner decision needed)

| # | Fix | Evidence |
| :--- | :--- | :--- |
| S1 | Block Get updates / git sync / external reload of a file that is live | `versionActions.ts` has no live guard |
| S2 | Joining with no host answering must not blank the file or show "Live"; add a "Waiting for host" state with a timeout and a clear error | `session.rs` sync deadline, `plugin.rs` replace |
| S3 | Keep a disposable local snapshot (sidecar in app data) so a document survives everyone leaving; rejoin an empty room from it | ADR 0013 decision 2 not implemented |
| S4 | Wire `Co-authored-by` trailers and member email into commits | `live.contributors` unused |
| S5 | Error state recovers; Diverged triggers an automatic resync or a clear action | `driver.rs` |
| S6 | Docs match code (ADR 0013 status, `edit-together-live.md` relay line, `vcs.rs`/`seed.rs`/`presence.rs` comments) | audit section 3 |

## Waves 1-4: project-wide sessions

Design summary (full reasoning in the agent report, reproduced as work packages):

* **Model.** One project doc (Loro movable tree keyed by stable file id) plus one
  Loro doc per text file, loaded lazily and evicted to a disposable sidecar.
  Binary and figure files are tree nodes plus content-addressed, chunked blobs.
* **Wire.** Envelope version and kinds unchanged (both relays untouched). New
  postcard `Message` variants appended (`DocSync`, `DocUpdate`, `Manifest`,
  `BlobGet`, `BlobChunk`), a separate room id for project rooms, invite v2;
  old single-file invites keep working.
* **Join.** Two phases: fetch manifest, classify local files (identical,
  missing, differing, guest-only), then resolve. Never silent, never a mass
  delete; replaced files go to `live-trash` first.
* **Disk.** Non-open files are written by the session thread (debounced, echo
  suppressed); open files keep the web view's autosave as the single writer.
  Every remote path is re-validated: an invite holder now writes to guests'
  disks, so this is a security boundary.
* **Presence.** Active file, open files, selection; follow mode is local state.
* **UI.** Per-file participant swatches on tabs and file rows (square, with
  names), participants and Follow in the Share popover, join/leave notices,
  sync progress. Design-system rules and the fixed vocabulary hold.

| WP | Scope | Weight | Depends |
| :--- | :--- | :--- | :--- |
| P1 | `manang-live` data model: `project.rs`, `blob.rs`, protocol/presence/envelope/seed v2, proptest tree convergence, wasm32 check | light (`-p manang-live`) | none |
| P2 | Multi-doc session router, paced anti-entropy | light-medium | P1 |
| P3 | Disk and tree bridge in the plugin crate, path validation | heavy | P1 |
| P4 | Plugin/driver, commands and events, two-instance tests through an in-process relay | heavy | P1-P3 |
| P5 | Frontend state and bindings (per-path), vitest | light | contract only |
| P6 | Frontend UI, verify-ui live section | light | P5 |
| P7 | Fuzzing, ADR 0014, relay conformance with bulk traffic | heavy, last | P4, P6 |

Schedule (at most two build-heavy agents at once): wave 0 together with P1;
then P2 with P5/P6; then P3 with P4; then P7; the lead runs the full section 5
gate on each merge result.

## Beyond project sessions (ranked)

1. Member identity and roles: per-member Ed25519 keys, a signed membership log,
   viewer / commenter / editor enforced by accept-time validation. Needed before
   any role is real; with one shared secret a "viewer" is a UI courtesy only.
2. Epoch-key revocation and expiring share links (wrapped room keys, rotate).
3. Anchored comment threads (Loro cursors, signed side doc) then suggestions.
4. Optional durable room: encrypted snapshot and updates stored at the relay
   behind a write capability, hash-chain rollback detection, self-hostable.
   Default relay stays stateless.
5. Version history and named versions (Loro checkout, restore as a new edit),
   with shallow-snapshot epochs for compaction.
6. Figures as a Loro map keyed by node id (ADR L9).
7. Deferred: iroh transport (blobs, LAN), chat, voice, MLS/BeeKEM.

## Owner decisions needed before wave 3 (P3/P4)

1. **Trust.** An invite holder can create, overwrite and delete project files on
   every guest's disk. Proposed deny-list: no dotfiles, `manang.toml`, `.manang/`,
   `.git/`; size caps. Read-only guests are not cryptographically enforceable
   until identity and roles exist.
2. **Destructive join.** Per-file "host wins, old copy to live-trash" or require
   a clean git tree / explicit all-files confirmation? (No three-way merge until
   `manang-merge` exists.)
3. **Guest-only files.** Proposed: keep private, ask once.
4. **Figures and binaries.** Last-writer-wins blobs are lossy for concurrent
   figure edits: accept for beta, or exclude figures until L9?
5. **Host leaving.** Warn, or block until a peer holds every file?
6. **Dead rooms.** The manifest comes from a live peer, so joining an empty room
   is impossible until the durable-room option. Acceptable?

## Risks

Seed identity silently diverges if fingerprints are wrong (per-doc digest check
and per-file re-fetch on Diverged); Loro memory per hot doc against the 75 MB
idle budget (cap loaded docs, evict); relay limits and Cloudflare's 20:1
WebSocket billing under bulk sync (pace the sender); case-insensitive file
systems and Windows names; git checkouts that rewrite hundreds of files during a
session.
