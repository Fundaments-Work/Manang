# ADR 0016: Plugin registry (signed index, mirrored artifacts)

Status: **accepted for the first version**, 2026-10-19. Plan:
`docs/plans/2026-10-19-plugin-marketplace.md`. Related: ADR 0003 (WASM plugin
runtime and capabilities), ADR 0007 (plugin settings), ADR 0014 (web version),
AGENTS.md invariants 4 and 5.

Note (2026-10-20): moving the registry's entries, schemas, validator and review tooling into a separate public repository, together with the plugin SDK, is proposed in ADR 0017 and not built. The in-tree `registry/` directory described here remains the current state.

## Context

Plugins are sandboxed WebAssembly components with declared capabilities
(ADR 0003). Installing one still required finding a `.manangplugin` and picking
it from disk. People expect to browse, read what a plugin may do, install and
update it from the app, the way Obsidian's community plugins or an editor's
extensions view work. A catalogue is also a supply-chain risk: whoever can
change what the app downloads can ship code to every user, and a plugin that
quietly asks for more access in an update is the classic abuse.

## Decision

1. **A hosted, static registry** (a Cloudflare Worker with R2 in front of the
   `registry/` folder's build output) serves four kinds of file: `index.json`
   (the catalogue, schema 1, frozen), `index.json.sig`, `stats.json` (download
   counters, display only) and the mirrored plugin archives and readmes. The
   app only ever issues plain GETs, with `If-None-Match` for the index. No
   identifier, account or installed-plugin list is sent.
2. **Trust root: an ed25519 signature over the exact bytes of `index.json`.**
   The app embeds a list of public keys (`crates/manang-registry/registry-keys`;
   several may be listed so a key can be rotated by shipping the new one before
   switching the signer). An index with no valid signature from any embedded key
   is refused, whatever else it contains, and the cache is left untouched. The
   signing key lives only as a secret of the registry's publish workflow.
   Verification uses `verify_strict`, which also refuses weak (small-order)
   keys and malleable signatures. Keys named `placeholder*` are never trusted,
   so a build that forgot to set a real key cannot be talked into anything.
3. **Every version is pinned by SHA-256 and size** in the signed index, and the
   archive is **mirrored** to the registry after review so a version cannot
   change or vanish when an author deletes a release (`source` records the
   author's release for provenance only). The app streams the download, stops at
   the listed size, compares the digest and discards the file on any mismatch.
4. **The consent text is the manifest's truth.** The index lists each version's
   `permissions`, exactly the manifest's `[permissions]` table. The Install
   dialog shows that list *before* anything is downloaded. After the download
   the app reads the archive's own manifest and refuses to continue unless it
   asks for exactly the permissions the index showed (equal as sets, per key),
   and the archive's name and version equal the index entry's. Then the same
   path as a hand-picked archive runs: stage, validate, compile and check imports
   against the manifest, install, approve with exactly the approved set. An update
   that asks for more is a new consent; the old version keeps running until it is
   given.
5. **Nothing installs by itself.** Updates are looked for when the Plugins view
   opens and in the background at most once a day, from the cached index with a
   conditional request; the result is a badge and a list. Every install or
   update is a click plus a consent.
6. **Offline and failure behaviour.** The last verified index is cached on disk
   (index, signature, ETag, time) and re-verified on every load, so a tampered
   cache file is ignored. Unreachable: the cache is served and the UI says "showing
   the list from <date>". Bad signature, malformed index or an unknown schema:
   an error with a plain sentence ("the plugin list could not be verified;
   nothing was installed"), no list shown. An index whose `generated` time is
   older than the cached one is refused (a cheap guard against a rollback to an
   older, signed list). Size and count limits bound every file.
7. **Switches.** The address comes from the build constant
   `MANANG_REGISTRY_URL` (https only; plain http only for loopback, for tests; a
   `.invalid` placeholder until the host is chosen). The user setting
   `plugins.registry-url` overrides it; `plugins.browse` (default on) and the
   policy gate (`PolicyAction::Browse`) switch the whole thing off, in which case
   no request is made.
8. **Web-capable core.** The client is its own crate, `manang-registry`, with the
   HTTP part behind a `Transport` trait (default-on `native` feature supplies it
   with ureq on rustls/ring) and the clock passed in as a parameter, so it
   compiles for `wasm32-unknown-unknown` and a browser build can supply `fetch`
   (invariant 5). The marketplace commands live in `manang-plugins`, which owns
   installation.

## Trust model, plainly

| Threat | What stops it |
| --- | --- |
| A compromised host serves a different index | The index must verify against an embedded key. The host has no key. |
| A compromised host serves a different archive | The index (signed) pins its SHA-256 and size. |
| A tampered local cache | The cache is verified again on load. |
| An archive that asks for more than the index said | The manifest is compared with the index before install; a mismatch installs nothing. |
| An update that quietly gains access | It parks beside the running version and needs a new consent. |
| An author deletes or replaces a release | Artifacts are mirrored and pinned. |
| A malicious plugin that was approved | It is sandboxed (ADR 0003): only the approved capabilities, limits on time and memory. Review is about honesty and quality, not the host's safety. |
| A leaked signing key | Not stopped: rotate by shipping a new key list in an app release. Existing installs are unaffected. |

## Consequences and follow-ups

- Readmes and `stats.json` are **not** covered by the index signature (the index
  carries no hash for them). The readme is rendered through the Markdown
  renderer that filters raw HTML and URLs, and counters only order and label
  rows. Follow-up proposal: add `readme_sha256` to each plugin in the next index
  schema.
- **No revocation list yet.** A takedown stops new installs and updates (the
  version leaves the index or is listed as withdrawn) but does not remove an
  installed copy. Follow-up (marked in `marketplace.rs`): a signed revocation
  list that disables a version on installed copies after a notice.
- The index schema carries no per-key id, so the app tries each embedded key in
  turn. That is fine for a handful of keys.
- A plugin installed by hand is outside these guarantees; the install dialog and
  the user guide say so.
- Review criteria for submissions are in `docs/plugins-publishing.md`, section 5
  (`registry/README.md` describes the entry format only).

## Rejected alternatives

- **Trust the host's TLS only.** One compromise of the host (or its
  credentials) would then push code to every user.
- **Sign each plugin archive separately.** Possible later, to allow third-party
  mirrors; the signed index already pins each archive by hash, which gives the
  same integrity for the same key-management cost.
- **Download from the authors' GitHub releases directly.** Authors can delete or
  replace assets, and there would be no place to pin a hash the app trusts.
