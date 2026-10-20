# Cryptographic design and threat model for live collaboration (C1-C7)

Date: 2026-10-20. Status: DRAFT, not reviewed. Source: a read-only design pass; claims are
[V] verified in code or a primary source (fetched 2026-10-20), [I] inferred, [U] unverified.
No code implements this yet. An adversarial review and a cryptographer's review of sections
3, 4 and 6 are required before C1 code (see "Review gates"). The wire contract is public;
the relay that serves it is a separate private repository.

## 0. Findings that shape the design

* [V] `envelope.rs` today: one shared secret S yields the AEAD key and the room id; its own
  header says there is no authentication between participants. `presence.rs` name and email
  are claims; `Message::Update{from: u64}` carries an unauthenticated peer id.
* [V] The relay checks length, version byte and kind (1 or 2); kind 3 is reserved; new
  behaviour arrives through upgrade parameters; the meaning of v1 frames never changes.
* [V] Dependencies already in the repo: chacha20poly1305 0.11, hkdf 0.13, sha2 0.11,
  zeroize 1, getrandom 0.4, postcard 1.1, loro 1.16. ed25519-dalek 3.0.0 and
  x25519-dalek 3.0.0 (stable, 2026-07-06) are compatible with that stack.
* Two deliberate breaks from "S is the key": the room id becomes self-certifying (section 5)
  and the invite no longer carries a long-lived data key (section 2).

## 1. Assets, adversaries, promises

Assets: document plaintext and history; integrity of who-may-write; authorship attribution;
membership truth; availability; device keys.

| Adversary | Can do | Defended by |
| :--- | :--- | :--- |
| Outsider with network access | read ciphertext, connect, flood | AEAD, caps, relay limits; a strict room refuses connections without a cap |
| Honest-but-curious relay | sees room id, sizes, timing, kinds, IPs, epoch number, role (via cap) | content, names and membership stay hidden; metadata is not promised |
| Malicious relay | drop, delay, reorder, duplicate, replay, roll back, truncate, equivocate | signed per-author updates, hash-chained log, per-author seq, checkpoints; detects forgery, reorder, replay, rollback or truncation relative to what a device has seen; cannot prevent drop, delay, or showing a new device a stale-but-valid past |
| Viewer who tries to write | holds the epoch key, can make valid ciphertext | no valid author signature as editor or commenter, so peers reject; the relay refuses frames from a read-only cap |
| Commenter who tries to edit text | signs as themselves | peers reject text-doc updates from non-editors; annotations go through a deterministic fold |
| Editor who forges another member's edit | has the room key | per-update signature and the peer-id derivation rule |
| Removed member | keeps old epoch keys and history | cannot read epoch >= e+1, cannot have new writes accepted past the cutoff; cannot be made to forget |
| Member replaying old log entries | has the old log | chain by height and prev-hash: old entries do not extend the head |
| Stolen device | holds signing key, X25519 key, known epoch keys | admin removal plus rekey; until then it is that member |
| Leaked invite link | holds invite_seed | one-time, expiring, role-capped capability; a second use is flagged |

Roles: a **viewer** reads everything in the epochs it holds and can leak it; a **commenter**
is a viewer plus signed annotation records; an **editor** edits under its own identity;
**admin** and **owner** change membership, rekey and issue tokens. A removed member keeps
everything they saw: cryptography cannot undo that.

We promise: (1) authorship of accepted updates is bound to a device key listed in a log every
peer verifies; (2) peers apply the same authorization decision to the same bytes; (3) a
removed member cannot read new epochs or have new writes accepted, subject to the cutoff rule.
We do not promise: forward secrecy of stored history; metadata hiding; freshness for a
brand-new device against a malicious relay; that a malicious owner cannot lie (the owner is
the root of trust); deniability; security on compromised endpoints.

## 2. Identity

* Each device has a random Ed25519 signing key and a separate X25519 agreement key (no Ed to X
  conversion). `kid = SHA-256("manang-live/kid/v1\0" || sign_pub)[..16]`. Storage: a new
  namespace in `manang-keystore` (OS keychain, then kernel keyring); with the in-memory
  fallback the identity is ephemeral and the UI says so. Seeds in `Zeroizing`; never logged.
* **Join (v3 invite).** The fragment carries `authority_pub(32) || invite_seed(32) ||
  manifest_root(16)`; no long-lived data key. The joiner verifies the genesis against
  `authority_pub` (no trust-on-first-use). The owner or an admin writes
  `InviteIssue{invite_pub, role, expires, uses=1}` to the log. The joiner sends a
  `JoinRequest{sign_pub, x_pub, claimed_name, invite_id, nonce}` signed by the device key and
  the invite key, sealed under `HKDF(invite_seed, "join-mailbox")`, in a mailbox frame. An
  admin sees the claimed name, device fingerprint, invite id, role and time, and approves or
  auto-admits per setting. The admin writes `Add` and sends a welcome (the log so far and the
  current epoch key, HPKE-style wrapped to `x_pub`). An invite is consumed by the first `Add`
  citing it in chain order; a second `Add` for it is invalid.
* **Names.** The displayed name is the admin-bound label in `Add`; a self-claim shows as
  "claims: X" and never replaces it; presence name and email are cosmetic. Fingerprint =
  base32 of `SHA-256("manang-live/fp/v1\0" || sign_pub || x_pub)[..20]` in groups of four; an
  admin can mark a member verified.
* **Devices and loss.** Each device is its own member entry; no cross-device certification in
  v1 (a second device means a second invite). Key loss: re-join with a new device and remove
  the old kid. A member may rotate its X25519 key with a self-signed `UpdateKey`.

## 3. Signed membership log

* postcard of a `#[derive]` struct with no maps and no floats. [U] postcard may accept
  non-minimal varints on decode, so the signature covers the exact received `body_bytes` and
  the verifier requires `encode(decode(b)) == b`; never re-encode for hashing.
* `body = {v:u8=1, ty, room_id[16], height:u64, prev:[32], epoch:u32, author_kid[16],
  ts:u64 (advisory only), nonce[16], payload}`;
  `sig = Ed25519("manang-live/log/v1\0" || body)` verified with `verify_strict`;
  `hash = SHA-256("manang-live/entry/v1\0" || body || sig)`.
* Payload types: `Genesis{owner keys, flags{strict_relay}, name}` (height 0, prev 0,
  `room_id = SHA-256("manang-live/room/v3\0" || authority_pub)[..16]`), `InviteIssue`,
  `InviteRevoke`, `Add{member keys, role, label, invite_id, history policy, wraps}`,
  `SetRole`, `Remove{kid, cutoff_seq, new epoch, wraps}`, `Rekey{epoch, wraps}`,
  `AdminGrant`, `AdminRevoke`, `Ratify{update_hashes (at most 32)}`, `Checkpoint{height,
  state_hash}`.
* Authority: the owner (genesis key) may do everything; an admin may Add, SetRole to viewer,
  commenter or editor, Remove non-admins, Rekey, issue and revoke invites, Ratify; only the
  owner grants or revokes admin or removes an admin. Ownership transfer is out of v1.
* Validation, a pure function `apply(state, entry) -> Result`: `v == 1`, room id matches,
  `height == head.height + 1`, `prev == head.hash`, author is a current admin or owner at
  `height-1`, signature verifies, `epoch` is current (or current+1 only in Remove/Rekey),
  and payload rules (Add needs an unconsumed, unexpired, unrevoked invite, role no higher
  than the invite's, a new kid, wraps for exactly the right recipients; Remove needs a member
  target and wraps for exactly the remaining members; the owner cannot be removed).
* Forks: two valid entries at one height conflict; the lowest `hash` wins; a peer that applied
  the loser reorgs to the fork point (the state function is pure); the last 8 heights are
  reorgable, deeper conflicts freeze and alert. Equivocation (same author, same height) is a
  self-verifying proof that freezes that author's authority until the owner revokes it.
  Ship owner-only authoring first; enable multiple admins once the fork proptests are green.
* A peer missing entries buffers out-of-order ones (at most 256 and 1 MiB) and sends
  `LogRequest{have_head}`. An admin-signed `Checkpoint` every 1000 entries lets a joiner
  start from the latest checkpoint and verify forward.
* Invariants to test: I1 any two peers fed the same entry multiset in any order and with
  duplicates agree on the lowest-hash branch; I2 no entry applies unless its author was an
  authorized admin at the previous height; I3 roles never exceed the author's authority; I4 an
  invite is consumed at most once; I5 the owner is never removable and a removed member is
  never a recipient of later wraps; I6 heights strictly increase along the winning chain; I7
  mutating any byte rejects the entry and truncation yields a valid prefix state; I8 replay is
  idempotent.

## 4. Authorization at accept time

* Inside the AEAD envelope: `SignedUpdate{doc_id, author_kid, author_seq:u64 (gapless per
  device), prev_hash, session_nonce:[u8;8], log_head:[u8;32], epoch:u32, payload (Loro
  update), sig}` with `sig = Ed25519("manang-live/upd/v1\0" || room_id || epoch || doc_id ||
  author_kid || author_seq || prev_hash || session_nonce || log_head || SHA-256(payload))`.
  Presence and every control frame are signed the same way under their own domain label.
* Loro `peer_id = u64_be(SHA-256("manang-live/peer/v1\0" || sign_pub || session_nonce)[..8])`;
  the acceptor recomputes it and requires every peer id the update advances to equal the
  author's derived id. [U] The Loro API for reading an update's peer ids without applying it
  is unchecked: a gating verification for C1. Grinding a victim's peer id costs about 2^64
  work (accepted residual). No first-claim map (it would diverge between peers).
* Cost: verify is about 50-80 microseconds [I]; per-update signatures are affordable.
* Rules (deterministic in the update plus the log): the author must be an editor at
  `log_head` (commenter or editor for the annotation doc); an unknown `log_head` goes to a
  bounded pending buffer and the log is requested (never dropped, so validity does not depend
  on arrival time); the epoch must be one the peer holds a key for; per-author gapless
  `author_seq` with the prev_hash chain; **removal cutoff**: `Remove` carries `cutoff_seq`, and
  an update from the removed member is accepted only if `seq <= cutoff` or its hash is in a
  `Ratify`. Backdating `log_head` is blocked by that rule, not by `log_head`.
* Hard case: an honest update depends on a quarantined op. Quarantined ops are never
  auto-accepted; the admin can `Ratify` specific hashes; honest peers report quarantined
  hashes they depend on; until then peers show "waiting for admin" and the digest divergence
  detector flags the state. Bounded divergence is accepted over unbounded forgery.
* Annotations (C5-C7) are signed records `{ann_id, kind, lamport, author, body}` stored as
  opaque elements of a reserved annotation doc, validated by a deterministic fold: a
  commenter may create and edit or delete only their own; an editor may resolve or delete any;
  text is never touched. A suggestion is a commenter record with an anchor and replacement;
  accepting it makes the editor sign a normal text update. Annotation updates inside the text
  doc, and text updates from commenters, are rejected.
* Sync replies are sequences of original `SignedUpdate`s (a replier's signature does not attest
  foreign ops); about 170 bytes per update, so 50k updates is about 8 MB. Snapshots are
  attested only by their signer, who must be an editor; attribution inside an imported
  snapshot is as reliable as that signer (stated residual).

## 5. Relay write tokens (C2)

* **Self-certifying room:** `room_id = H(authority_pub)[..16]`; the relay verifies
  registration statelessly, and squatting needs a second preimage. Path `GET
  /room3/<id>?cap=<token>`; legacy `/room/<id>` stays open and lives in a different id space.
  A `/room3` connection without a valid cap is refused. A strict room's genesis sets
  `strict_relay`; a strict client refuses a relay that does not answer `x-relay-caps:
  enforced` (a lying relay can only deny service).
* Token: `cap = base64url(v:1 | room_id | role:{r,w} | exp:u64 | nonce:16 | kid:16 |
  issuer_pub:32 | cert? | sig)`, `sig = Ed25519("manang-live/cap/v1\0" || fields)`. The issuer
  is the owner, or an admin with a delegation `cert{admin_pub, exp, serial}` signed by the
  owner. The relay checks `H(authority_pub) == room_id`, cert signature, token signature,
  expiry, role. Role `w` may send; a commenter token is `w` (the relay cannot see the doc).
  Expiry 1 hour, renewed in-band by an admin-signed refresh; cert expiry 24 hours. A removed
  admin's cert survives until expiry; log-level removal is the real enforcement.
* The `cap` query parameter must never be logged (add a test that scans logs for it).
* The relay refuses read-only connections' frames and unauthenticated connections; that
  reduces spam and viewer writes at the network layer but is not an integrity guarantee,
  because peers re-check everything and a malicious relay can ignore it.
* Wire changes (relay-proto, WIRE.md, vectors): `/room3/<id>` with `cap`; response header
  `x-relay-caps: enforced`; envelope v2 `2 | kind | epoch:u32 | nonce(24) | ct` with the epoch
  in the AAD, negotiated by an upgrade parameter such as `proto=3`; kind 3 (epoch
  announcement); close code 4001 "stale epoch"; kind 4 (join mailbox), rate-limited.

## 6. Epochs and removal (C3)

| Option | Verdict |
| :--- | :--- |
| **Admin-signed epoch keys wrapped per member** | **Chosen**: O(n) with n at most 20; a rekey is a log entry, so the chain orders it |
| MLS via OpenMLS | OpenMLS 0.9.1 (2026-10-07) exists [V]; RFC 9420 assumes a delivery service that orders Commits [I], which a dumb relay cannot; needs an authentication service and KeyPackage distribution, no roles; forward secrecy and post-compromise security in O(log n) not needed at n <= 20; audit status [U]; revisit above about 50 members |
| Keyhive / BeeKEM | README says pre-alpha, not for production, unaudited [V, github.com/inkandswitch/keyhive, 2026-10-20]; rejected for now |
| Sender keys | ratchets conflict with offline members, reordering and CRDT re-reads; rejected |

* Wrap: ephemeral X25519 against the member's `x_pub`, `HKDF-SHA256(salt = room_id || epoch,
  info = "manang-live/wrap/v1\0" || eph_pub || member_pub)`, XChaCha20-Poly1305, AAD =
  `room_id || epoch || kid || entry_body_hash`. Prefer the `hpke` crate (RFC 9180, DHKEM-X25519)
  if its dependencies align [U]; otherwise this composition with known-answer tests. A new
  epoch key is fresh random 32 bytes, never derived from the previous key.
* `Remove` and `Rekey` carry `new_epoch = epoch+1` and the wraps; epochs are totally ordered by
  the log chain; concurrent rekeys resolve by the fork rule and the loser's key is discarded.
  Kind 3 frame: `epoch | admin-signed {room_id, epoch, entry_hash}`; the relay sets an epoch
  floor and refuses lower epochs with 4001 after a 30-second grace. Offline and late members
  fetch log entries and wraps from a peer or the durable store and keep a key ring.
* Properties: a removed member without other leaks cannot decrypt epoch >= e+1; no forward
  secrecy within an epoch or for stored history; post-compromise security only through removal
  or `UpdateKey` (a compromised long-term X25519 key reads every future wrap addressed to it
  until then). Cost per rotation: n X25519 operations, HKDFs and AEADs, well under 5 ms; about
  2 KB for 20 members.

## 7. Durable rooms (C4)

* The relay stores an append-only sequence of opaque sealed frames per room; writes need a `w`
  cap, reads an `r` cap; sizes, counts and times are visible.
* Rollback and truncation evidence inside the ciphertext: per-author `author_seq` and
  `prev_hash` chains; editor-signed `Checkpoint{epoch, per-author heads, log_height, snapshot
  hash}` about every 5 minutes and on leave; each device persists a local high-water mark.
  Detected: a store older than a device's mark, a gap in an author chain, tail truncation of an
  author whose own device remembers a higher seq. Not detected: a stale-but-valid state shown
  to a brand-new device with no mark (impossible with a dumb relay); mitigation: ask live peers
  for heads and compare checkpoints from several sources.
* Quotas and retention: 64 MiB per room, 90 days idle, an admin-signed delete token, the last
  three snapshot generations kept. Sealed snapshot: `{epoch, covers_vv, log_height,
  signer_kid, sig, XChaCha20-Poly1305 ct}`; the relay cannot verify the signer, so a malicious
  editor can upload garbage (prior generations limit the damage).
* History across epochs: a new member receives the current snapshot; older epoch keys only if
  the admin chooses (`history: current | all`). Removal never erases history.

## 8. Libraries and encodings

Ed25519 `ed25519-dalek 3.0.0` with `verify_strict`; X25519 `x25519-dalek 3.0.0`;
`chacha20poly1305 0.11`, `hkdf 0.13`, `sha2 0.11`, `zeroize`, `getrandom 0.4`, `postcard 1.1`
plus the re-encode check. Audit status of the dalek crates is [U]: check RustSec before
pinning. Domain-separation convention `"manang-live/<purpose>/v<n>\0"` as the first bytes of
every signed or KDF input. Do not invent a KEM, an AEAD, a ratchet, a signature context scheme,
a hash chain other than SHA-256 over bytes, or a canonical form other than "signed bytes are
the wire bytes".

## 9. Test plan

Property tests: invariants I1-I8; convergence of the valid-update set under any delivery
order; validity independent of arrival time; accounting returns to zero after leave. Fuzz
targets (60 s each, clean): SignedUpdate, log entry, join request, welcome, cap token, kind 3
and 4 frames, checkpoint. Known-answer vectors: signatures per domain, entry hashes, wrap and
unwrap with fixed ephemerals, cap tokens, envelope v2, v3 fragment encoding.

Adversarial simulations (each with its expected outcome): 1 viewer sends a signed text update
(rejected; relay drops it); 2 viewer forges an editor signature; 3 commenter edits the text
doc; 4 commenter deletes another's comment (fold rejects); 5 editor forges Bob's update with
Bob's peer id (mismatch); 6 editor signs with Bob's kid (signature fails); 7 removed member
sends seq > cutoff (rejected everywhere); 8 removed member sends seq <= cutoff after removal
(accepted everywhere); 9 removed member backdates `log_head` (rule rejects); 10 removed
member tries to read a new epoch (cannot decrypt); 11 replay of an old log entry (no change);
12 replay of an old SignedUpdate (duplicate ignored); 13 two admins act at the same height
(lowest hash wins everywhere); 14 admin equivocates (proof, authority frozen); 15 relay
reorders log entries (buffered, applied in order); 16 relay drops an update (gap, resync);
17 relay truncates the tail (detected by a device with a higher mark); 18 relay rolls back
durable storage (detected below the mark); 19 relay replays an old epoch's frames (refused,
4001); 20 relay shows two peers different log branches (exposed by head exchange); 21 leaked
invite used twice (second Add invalid, flagged); 22 expired invite; 23 mismatched invite
signature; 24 stranger on `/room3` with no cap (refused); 25 expired or tampered cap; 26 cap
for the wrong room; 27 cap token in logs (scan finds none); 28 relay without `x-relay-caps`
(strict client refuses); 29 non-canonical postcard bytes (rejected); 30 pending-buffer
exhaustion (bounded, no OOM); 31 unknown `log_head` (pending, not dropped); 32 quarantined
dependency ("waiting for admin" until Ratify).

Budgets (proposed, to measure): signature verify under 100 microseconds each (under 2% of a
core at 100 updates/s); log verify for 20 members and 10,000 entries under 1.5 s on one
thread (batch verify to cut further); member state under 64 KB, log under 4 MB, pending
buffer capped at 1 MiB, signed-update store about 8 MB per 50k updates; keystroke to screen
p99 under 8 ms with a live session; a two-hour accelerated soak with under 5% RSS growth.

## 10. Open questions and residual risks

Owner decisions: owner-only or multi-admin authoring first (recommended: owner-only);
auto-admit on a valid invite or always ask (recommended: ask for editor, auto for viewer);
default history policy for new members; v3 rooms coexist with v1 and v2 (recommended) or
replace them; keep the legacy open mode after v3 ships; durable-room quota and retention
defaults; adopt the `hpke` crate if its dependencies align, or hand-compose the wrap.

Residual risks: a compromised owner key is total compromise (new room only); host-key loss
means no new tokens or membership changes; peer-id grinding at about 2^64 work; attribution
inside snapshots rests on the signer; quarantined ops can stall honest peers until ratified;
a removed member keeps what they saw; a malicious relay can deny service and show a new
device a stale state; metadata is visible (room id, timing, sizes, roles via cap); traffic
analysis of join and rekey events; the Loro metadata API for peer-id checks is unverified.

## Review gates

1. Now, before C1 code: an adversarial review by a fresh agent, then a short review by a
   cryptographer focused on sections 3, 4 and 6.
2. After C3 is code-complete and before any marketing of roles or removal: a full external
   review of C1-C3 and the relay token code.
3. Until then "remove member" is not presented as secure and the moat sentence is not advertised.
