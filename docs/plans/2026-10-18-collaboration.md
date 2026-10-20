# Collaboration: design and work packages

> Note (2026-10-20): The relay server moved to a separate private repository on 2026-10-20 (`crates/manang-relay` and `relay/worker` no longer exist here; the app keeps `crates/manang-relay-client`). Paths below describe the state when this was written.

Date: 2026-10-18. Refines "Collaboration: git-based first" in
`2026-10-18-roadmap.md`. Facts verified on 2026-10-18 on crates.io and primary
docs; unverified items are marked.

## Baseline

`manang-git`: reads through gix 0.89; writes through a hardened system `git`
subprocess (credential helper and `SSH_AUTH_SOCK`, `GIT_TERMINAL_PROMPT=0`,
`ssh -o BatchMode=yes`). Pull is `--ff-only`; push never forces. No merge,
restore, remote-add or conflict commands yet. Pitfalls: git missing; passphrase
keys without an agent; HTTPS needs a credential helper; first connection to a
host fails under BatchMode (the UI needs a one-time "trust this host" step).

## Phase A: git-based collaboration (build first)

**Confirmed by the owner, 2026-10-18:** the default vocabulary for
non-developers is "Save a version / Share / Get updates / Versions", with the
full Source Control view as an advanced mode; build starts with WP1 (Versions
and restore).

**Two layers over the same git commands.** Simple layer for non-developers:
Save a version (stage all non-ignored files and commit with a name), Share
(push the current branch), Get updates (fetch then merge), Versions (history
with diff and Restore), Conflicts (resolver); branches shown as "Drafts".
Advanced layer: the Source Control view. Overleaf's git integration loses
comments on round trip and has no branches; ours is real git with comments as
files.

- **Sync commands:** `git.remote-list`, `git.remote-add {name,url}` (validate:
  https/ssh only, no `ext::`, no embedded password; policy allow-list),
  `git.sync-status`, `git.sync {action, remote?, strategy: ff-only|merge}`
  returning `conflicts` when a merge stops, `git.credential-hint`. No rebase for
  non-developers (it breaks comment anchors and ids). Never auto-pull or
  auto-push; background fetch only opt-in under policy.
- **Identity:** a "Your name and email" dialog writes `user.name`/`user.email`
  instead of failing the first commit; signing is a policy-controlled opt-in.
- **Merge:** use the system git for the merge itself, take base/ours/theirs from
  the index, and build regions with a small in-house `merge3` over `similar`
  (already a dependency) returning `Stable | Conflict{base, ours, theirs,
  word_diff}`; word-level refinement makes one-paragraph-per-line prose
  readable. `diffy` 0.5.2 and `gix-merge` 0.22 do not (verifiably) expose
  structured conflict hunks: use them only as test oracles together with
  `git merge-file -p --diff3`. Properties: either side equals base gives the
  other; ours equals theirs gives either; non-overlapping edits are
  order-independent.
- **Figures and binaries:** figure scenes merge by node id (added on one side
  kept; removed on one side removed unless modified on the other; same node
  modified on both: per-field merge with a keep-mine/keep-theirs choice shown as
  overlaid previews); scene JSON must serialise deterministically (sorted keys,
  one node per line). Binary files: keep mine or theirs, never text-merge;
  generated outputs ignored or `merge=ours`. Copy all three sides to
  `<git-dir>/manang-trash/` before any resolution write.
- **History and restore:** `git.restore {rev, paths, whole?}` via
  `git restore --source` (never rewrites history; refuse on a dirty tree or save
  a safety version first). Local non-git history snapshots live in the app data
  dir; only "Save a version" makes a real commit.

## Comments and suggestions as files

One immutable file per event: `.manang/comments/<thread-id>/<event-ulid>.toml`
plus a once-written thread header. Appends to one file conflict in git; one file
per event is conflict-free by construction, and status derives from the latest
resolve/reopen event by ULID order. Authors come from git identity (a claim, not
authentication unless commits are signed). Private notes live in a local-only
`.manang/notes/`.

Anchoring (W3C TextQuoteSelector plus a position hint, as Hypothesis does):
exact position check, then `prefix+exact+suffix`, then fuzzy search near the
hint, then global fuzzy; below threshold the thread is "Detached" with its
original quote and can be re-attached (never silently deleted). In the editor,
map anchors through each transaction with `ChangeSet.mapPos`; re-anchor on file
open, after Get updates and after external changes. Suggestions are comments
with `{exact, replacement}`; accept applies an undoable transaction after
re-anchoring. Figure anchors use node ids; PDF anchors prefer the source
position (SyncTeX or Typst mapping) with page and rect as a secondary selector.
UI: a 2 px gutter marker and a tonal tint on the range, a "Review" mode of the
existing panel area (no new activity-bar noun), Editing/Suggesting/Viewing
toggle in the menu, "Accept all" as the single lime action.

## Phase B: real-time co-editing (plan only)

Spike **Loro** (pure Rust core, shallow snapshots, movable tree/list/map for the
figure scene, MIT) against **yrs** (battle-tested `y-codemirror.next`, MIT);
Automerge is a weaker fit. Versions on 2026-10-18: yrs 0.28.0, Loro 1.16.2,
Automerge 0.12.0 (benchmarks cited are the vendors' own; CodeMirror binding
maintenance for Loro and Automerge unverified). Keep the rope as the editor
buffer and bridge rope diffs to the CRDT; measure against the 8 ms budget.
Figures: a CRDT map keyed by node id plus an ordered id list; undo stays
per-user. The file on disk stays plain UTF-8: CRDT state is a disposable
sidecar; a live session seeds from the current file (after a three-way merge
against the git base if peers differ); commits list contributors and use
`Co-authored-by:` trailers; "Get updates" is disabled for files in a live
session. Transport behind a `SyncTransport` trait: a self-hostable WebSocket
relay first (works in browsers, one allow-listed host), iroh 1.3 direct
connections later (desktop; browsers only via relay; C-library footprint
unverified: `cargo tree` first). Room secret in the invite link fragment,
application-level AEAD (XChaCha20-Poly1305) so the relay sees only ciphertext;
presence and cursors are ephemeral. First milestone: one Markdown or Typst file,
two clients, one relay, encrypted updates, remote cursors, p99 keystroke latency
under 8 ms, convergence under a partition property test.

## Identity, policy, enterprise

Identity is git's. Access control is the remote's (and the room secret for
live sessions). Policy keys: `collab.sharing`, `collab.allowed_remotes`,
`collab.relays`, `collab.require_signed_commits`, `collab.live`; `remote-add`
and `sync` consult policy before touching the network (build a permissive
`policy.check` seam now). Audit trail is git history plus comment events;
"Export review log" as JSON/CSV. No Manang-operated service, no telemetry.

## Priorities

1. Versions with diff and Restore. 2. Identity, remote setup, Share / Get
updates. 3. Comments with anchors. 4. Conflict resolver. 5. Review mode with
suggestions. 6. Live editing spike, then milestone, then figures.

## Work packages (disjoint files)

- **WP1 Versions and restore** (`crates/manang-git` `restore.rs`,
  `identity.rs`; UI `src/lib/views/versions/*`): `git.restore`,
  `git.identity-get|set`, `git.commit` `trailers`. Temp-repo kernel tests;
  verify-ui. No dependencies.
- **WP2 Merge and conflicts** (new `crates/manang-merge`, pure Rust, wasm32;
  `manang-git` `merge.rs`): `merge3`, `merge_figure`; `git.sync strategy:merge`,
  `git.conflicts`, `git.conflict-sides`, `git.resolve`, `git.merge-abort`.
  proptest, git oracle, insta, fuzz (60 s cap).
- **WP3 Share and Get updates** (`manang-git` `remote.rs`): remote and status
  commands, host-trust step; needs the policy seam (package 6 of the shell plan).
- **WP4 Comments and suggestions** (new `crates/manang-annotations`, UI
  `src/lib/review/*`, `src/lib/editor/review.ts`): event store, thread
  assembly, `reanchor`, suggestions; `comments.*` commands; proptest that
  anchors survive edits and that two users' branches merge without conflict.
  Needs the menu bar for the Review menu.
- **WP5 Live spike** (`crates/manang-live`, `crates/manang-relay`): transport
  trait, relay, Loro vs yrs benchmarks on the rope bridge, wasm32 and
  `cargo tree` audit, AEAD envelope, convergence test; ships only after the
  policy layer.
Order: WP1, then WP2 and WP3 in parallel, then WP4; WP5 independently.

## Risks

Whether `gix-merge` exposes hunks (30-minute test before WP2); CodeMirror
binding maintenance; rope bridge cost; iroh's C footprint; git environment
variance (missing git, `autocrlf`, Windows paths); comment drift after heavy
rewrites; coarse merges of one-paragraph-per-line prose; the web target has no
subprocess git (keep logic behind traits); the commit-while-live rules need a
user test.
