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

## Owner decisions (resolved 2026-10-20: "yes do the best", proposed defaults accepted)

1. **Trust.** Remote paths are validated and the deny-list applies: no dotfiles,
   `manang.toml`, `.manang/`, `.git/`, no `..`/absolute/symlink escape; size caps.
   Read-only guests stay a UI courtesy until identity and roles exist.
2. **Destructive join.** Per file, the host's version wins and the guest's old
   copy goes to `live-trash` first; a join that would replace more than one file
   needs one explicit all-files confirmation. No three-way merge until
   `manang-merge` exists.
3. **Guest-only files.** Kept private; asked once whether to share.
4. **Figures and binaries.** Last-writer-wins blobs for the beta, documented as
   lossy for concurrent figure edits until the Loro figure model (L9).
5. **Host leaving.** Warn ("N files not yet synced to others"); do not block.
6. **Dead rooms.** Joining an empty room is not possible until the durable-room
   option; the UI says so plainly (S2).

## Risks

Seed identity silently diverges if fingerprints are wrong (per-doc digest check
and per-file re-fetch on Diverged); Loro memory per hot doc against the 75 MB
idle budget (cap loaded docs, evict); relay limits and Cloudflare's 20:1
WebSocket billing under bulk sync (pace the sender); case-insensitive file
systems and Windows names; git checkouts that rewrite hundreds of files during a
session.

## Status (2026-10-20)

Merged on `feat/live-collab-v2` (not yet on `main`): wave 0 (S1-S6), P1
(project tree, blobs, manifest, wire v2), P5 (frontend state), P6 (UI; verify-ui
live section 96/96 re-run on the merge) P2 (multi-doc session router) and P8 (frontend speaks the real plugin contract, leave warning, held-change dialog, plain errors; verify-ui live 106/106 run by the agent, frontend gates re-run on the merge), P4 (plugin and project driver; 70 + 18 + 12 plugin tests, tauri crate compiles) and P3 (path policy, disk and tree bridge, sidecar,
join classification; tests green on the merge, clippy clean). The
single-file driver is untouched; P4 (below) adds a separate project driver. Gates on the merge result: fmt, clippy
and tests for `manang-live`, `manang-live-plugin`, `manang-git`; `pnpm run check`,
vitest (1051), eslint. Build note: worktrees with the same layout must not share
one `CARGO_TARGET_DIR` at the same time (artifact names collide).

## Contract for P2-P6 (commands and events)

Existing, extended with `path`: `live.start {scope: "file"|"project", doc?}`,
`live.join {invite}` (phase 1: connect and fetch the manifest only),
`live.inspect` (returns scope), `live.push {path, ...}`,
`live.selection {path, ...}`, `live.snapshot {path}`, `live.status` (adds scope,
file count, progress), `live.leave`, `live.resync`, `live.contributors`.
New: `live.resolve {choices}` (apply the join plan), `live.attach {path}` ->
`{version, text}`, `live.detach {path}`, `live.follow {peer|null}` (local),
`live.files` (tree with per-file peers), `live.exclude {path}`,
`live.include {path}`, `live.restore {path}`.
Events: `live.remote` (+`path`), `live.presence` (+`path`), `live.state`, and new
`live.plan` (classified files: identical, missing, differing, guest-only),
`live.tree {ops: [{op, path, from?, by}]}`, `live.peers {peers: [{peer, name,
slot, path}]}`, `live.progress {done, total}`, `live.notice {kind: joined | left
| deleted | renamed, peer, path?}`. Single-file invites keep today's behaviour
(`scope: "file"`).

### Payload shapes (fixed by P5, binding for P3/P4)

* `live.plan` event: `{files: [{path, class: "identical" | "missing" | "differing" | "guest-only"}]}`.
* `live.resolve {choices: [{path, action: "take-host" | "keep-mine" | "share" | "keep-private"}]}`;
  the wildcard path `"*"` is the single all-files confirmation. A differing file
  with no choice is refused with code `unresolved`.
* `live.files` returns `{files: [{path, kind: "text" | "binary", peers: string[]}]}`.
* `live.peers` event: `{peers: [{peer, name, slot, path}]}`.
* `live.push`/`live.selection` require `path`; `live.snapshot {path}`;
  `live.start {doc?, file?, scope?}`; `live.inspect` returns `scope`.
* The plugin manifest must list the 8 new commands that `mockLive.test.ts` lists.
* Mock dev hooks for verify-ui: `dev.live-seed`, `dev.live-local`,
  `dev.live-make-invite`, `dev.live-peer-join|open|type|caret|create|rename|delete`,
  `dev.live-text`.

## P4: the plugin and the project driver (2026-10-20, branch `agent/live-p4-plugin`)

`crates/manang-live-plugin`: `projdrv/` is the project session thread (one OS
thread per session, like the single-file driver, which is unchanged); `scan.rs`
builds the host's tree from the folder through the path policy. Everything in
the contract above is implemented; what a shell or the UI package needs to know
beyond it:

* **The project folder.** The plugin has no way to learn it by itself (the
  project plugin exposes no "current root"). `live.start` and `live.join` take
  an optional `root` (absolute path of the open project folder, the `root` of
  `project.open`'s summary); `LivePlugin::with_project_root(|| Option<PathBuf>)`
  is the alternative for the shell. Without either, a project command fails
  with code `no-project`. The frontend's `liveApi.start/join` must pass `root`.
* **`live.resolve {choices}`.** Per file: `take-host` (a differing file is
  replaced; its local copy goes to `.manang/trash` when the host's content
  arrives, nothing is touched before), `keep-mine` (a differing file stays out
  of the session on this side: never offered, never overwritten, not listed),
  `share` (a guest-only file joins the room), `keep-private` (default for
  guest-only files, also with no choice). The wildcard `"*"` is the single
  all-files confirmation: its action covers every differing file without an
  explicit choice (explicit choices win). Refusals: `unresolved` (a differing
  file with no choice and no wildcard) and `needs-confirmation` (more than one
  replacement without `"*"` carrying `take-host`, even when every file was
  chosen explicitly: this is stricter than the mock, which allows it). `share`
  on a differing file is `invalid`. It returns `{applied, trashed}` where
  `trashed` lists the paths that will be replaced. It works before the tree
  has reached the guest (it goes by the manifest).
* **Phases.** `live.status.phase` is `manifest` (guest waiting for the host),
  `plan` (waiting for `live.resolve`) or `live`. Project commands other than
  `live.resolve`/`live.leave` are refused before `live` (`waiting`, or
  `invalid` while a plan waits). A resolve with no plan is `no-plan`.
* **`live.attach {path, doc?}`** waits (up to 8 s, then `waiting`) for a file
  still being fetched. `doc` is the kernel editor document that shows the file
  (optional; found by path otherwise); when given or found, remote edits are
  mirrored into it with `replace_silent`. The file's text on attach is the
  room's: the view must show it. A file open in a tab is written by the web
  view only; closing the tab (`live.detach`) writes the room's text to disk.
  A replaced file that is already open in a tab must be re-attached by the UI.
* **Host leaving (owner decision 5).** `live.status.unsynced_files` and the
  `live.peers` event's `unsynced_files` count the files (not folders, not files
  kept out) that no *present* peer has shown it holds: a peer holds a text
  file when it answered for it, was sent its state, confirmed it, or edited
  it; a blob when it was served to it. With nobody else present it equals the
  file count. The UI shows "N files not yet synced to others" before the host
  leaves; leaving is never blocked. Leaving never deletes anything on disk.
* **New events beyond the contract.** `live.paused {files, changed_lines,
  total_lines}` (a large rewrite on disk, such as a checkout, is held and not
  shared) answered with `live.resync {paused: "share" | "discard"}`; discarding
  rewrites the disk from the session (the rewritten copies go to the trash).
  `live.status` also has `paused`, `skipped` (paths left out or refused, with
  the reason), `phase`, `following`, `rejected`. `live.tree` ops carry `by`:
  exact for a create (the node's creator), otherwise the only other person
  present or `""`. `live.progress` is throttled to a few per second plus the
  last. `live.inspect` returns `scope`.
* **Limits.** A document parked in the sidecar misses live updates; it catches
  up when the editing peer's size/hash update arrives (about 1.5 s after the
  typing pauses) or when it is loaded again, so a peer that edits and crashes
  within that window leaves others' copies of that file behind until the next
  edit. Binary and figure files are last-writer-wins. An attached file edited
  outside the editor is the editor's business (no event). A blob is read from
  disk on the session thread when a peer asks for it.
* **Additive changes elsewhere.** `manang-live`: `Session::file_holders(id)`.
  `manang-live-plugin`: `DiskBridge::cancel_delete`, `InviteScope`,
  `LivePlugin::with_loaded_docs`/`with_project_root`.

## Gate result (2026-10-20, merge of all packages, branch `feat/live-collab-v2`)

Run once, alone, capped (4 build jobs, nice, ionice, 10 GB memory cap): `cargo fmt
--all --check`, `cargo clippy --all-targets --all-features --locked -D warnings`
(whole workspace), `cargo test --all --locked` (1,553 passed, 0 failed, 6
ignored), `pnpm run typecheck`, `pnpm run lint`: all exit 0. `cargo tree -p manang`
shows none of tectonic, harfbuzz, freetype-sys, openssl-sys, aws-lc.
`cargo check -p manang-live --target wasm32-unknown-unknown` passes. Not run: the
criterion benchmarks, the full UI suite beyond the live section (96/96 and
106/106 by the agents; 96/96 re-run by the lead before P8), the Cloudflare Worker
conformance, and two real windows against the hosted relay.
Fuzzing (P7, 60 s per target, real libFuzzer): seven targets clean; two found
bugs, fixed (a Loro cursor crash reachable by any invite holder) or documented
(a yrs panic; yrs is test-only).
