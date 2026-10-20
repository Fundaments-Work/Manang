# Plugin SDK: a developer product, not a monorepo clone

Date: 2026-10-20. Owner direction: separate the SDK, follow the Obsidian approach,
nobody forks or clones Manang to write a plugin. Sources: a read-only ecosystem study
(Obsidian, Zed, VS Code, Raycast, Extism/Spin) and a read-only extraction design of
this repository. [V] verified in a primary source or in the repo, [I] inferred, [U]
unverified. The relay is a different, private project and is out of scope here.

## What the ecosystems teach (verified)

| | Obsidian | Zed (closest: Rust to WASM, WIT) |
| :--- | :--- | :--- |
| Scaffold | Template repo (0BSD): build config, lint, version-bump script, release workflow, `AGENTS.md` | None official |
| SDK | npm `obsidian` 1.14.4, **types only**, no mock host | crates.io `zed_extension_api` 0.7.0, bundles the WIT |
| Dev loop | watch build, reload the app by hand | "install dev extension" from a directory, override banner, foreground log |
| Submission | Web dashboard since 2026-05-12; GitHub release with `main.js` and `manifest.json` | PR to `zed-industries/extensions`, git submodule, CI licence allow-list, 3-week reply rule |
| Review | Automated scan of **every version** (policy, code quality, vulnerabilities, malware), minutes; humans only for popular, featured, flagged | Human for every PR, weeks to 2 months |
| Versioning | `minAppVersion` plus `versions.json` | One WIT directory per API version; host keeps old ones; README compatibility table (it lags) |
| Known pain | Review queue of months before automation | Version drift between crate, WIT and host |

Also verified: Obsidian plans capability declarations shown before install (Manang's manifest
already declares them, so Manang can ship this first); VS Code's verified-publisher rules and
2026 supply-chain incidents (a scanner that failed open, evil-twin and namespace-resurrection
attacks) show that scanners must fail closed and names need proof of ownership. Component
toolchains: Rust is the safe first language; `jco` (JS) and `componentize-py` are documented
upstream; `cargo-component` calls itself experimental; none was built by us. WASI 0.3 shipped
in 2026 but guest toolchains mostly target 0.2.

## Decisions (recommended defaults; the owner confirms the ones needing accounts)

1. **Repositories** under `Fundaments-Work`, all public except the relay:
   `manang-sdk` (WIT source of truth, `manang-plugin` crate, `manang-plugin-test`,
   `cargo-manang` CLI, GitHub Action, examples, developer docs), `manang-plugin-template`
   (GitHub template, 0BSD), `manang-plugins-registry` (entries, schemas, scripts, review
   guidelines, policies). `fundaments-relay` stays private and separate.
2. **Licences:** MIT OR Apache-2.0 for the SDK and examples (authors link it into their
   plugins); 0BSD for the template; Manang stays MIT.
3. **Publish the crates** to crates.io (without it authors need git dependencies, which
   breaks "never clone"): `manang-plugin`, `manang-plugin-test`, `cargo-manang`. Add a
   second owner on crates.io. Names are checked again at publish time (`manang-sdk` was free).
4. **Single source of truth for the interface:** the WIT lives in `manang-sdk/wit/`.
   Manang keeps a vendored, read-only copy pinned by `SDK_PIN` (tag and sha256 per file),
   a test fails on drift, and builds stay offline. No git dependency in the app build.
5. **Interface versioning:** `api = "0.2"` in `[plugin]` of `manifest.toml`; absent means
   `0.2` with a warning (existing installs keep loading). Checked at install and at load.
   The host supports the current and the previous line; a removal needs at least two
   app releases or six months with warnings. Per-version WIT directories in the host only
   when a second version exists. Declare 0.x unstable now; freeze 1.0 after about ten
   third-party plugins and after settings and the visual-editor format interface settle.
6. **Languages:** Rust first-class (crate, CLI, template, tests). JS (`jco`) and Python
   (`componentize-py`) are "preview": validate and pack work, docs only, nothing promised
   until a hello-world is built against our pinned Wasmtime.
7. **Review:** automated on every version and fail closed (manifest, `api`, SPDX licence,
   archive limits, wasm imports versus declared permissions, no WASI imports); humans only
   for a first submission, a newly requested capability, popularity or a flag; a published
   review time target and auto-closing stale submissions; reserved ids with proof of
   repository ownership. Updates that add a capability show the diff to the user (already
   built). Revocation (a signed list the app checks) is **not built** and must be built
   before a public launch; until then a takedown only stops new installs, and the policy
   says so.
8. **Dev loop:** `cargo manang dev` rebuilds and atomically copies into the plugins folder;
   the host's existing hot reload (250 ms debounce) does the rest. A separate dev folder
   needs a small host setting (decide before the CLI).

## The author journey (target: zero to published in 30 minutes)

1. `cargo install cargo-manang`, then `cargo manang new my-plugin` (or the GitHub template).
2. `cargo manang build` produces the component; the docs say the first build is slow.
3. In Manang: install the dev plugin from the folder; it loads with a banner and a log.
4. Write a command; `cargo test` runs against the fake host (virtual filesystem, recorded
   HTTP, permission checks mirroring the real host).
5. `cargo manang validate` runs exactly the registry's checks.
6. Tag `v0.1.0`; the template workflow builds, packs and attaches the `.manangplugin`.
7. `cargo manang publish` prints (or opens) the registry entry; automated review runs;
   plugins asking for no risky capability pass without a human.

## What exists today and where it goes (verified in the repo)

| Today | Destination |
| :--- | :--- |
| `crates/manang-wasm-host/wit/plugin.wit` and two byte-identical copies in `sdk/rust-plugin-template` and `plugins/word-count` (the only guard compares those three) | `manang-sdk/wit/`; Manang keeps one vendored copy plus `SDK_PIN` |
| `sdk/rust-plugin-template`, `sdk/componentize`, `scripts/build-plugin.sh` (only works inside the Manang repo) | template repo; CLI library; `cargo manang build` (a 5-line shim during transition) |
| `plugins/word-count` | `manang-sdk/examples/word-count` (also the SDK's acceptance fixture) |
| `registry/` (schemas, scripts, Worker, tests, workflow) | `manang-plugins-registry`; its tests depend on `plugins/word-count/manifest.toml`, so vendor that file |
| `crates/manang-registry` (client, embedded production key) | stays; index schema 1 and the ed25519 signature are frozen |
| `docs/plugins.md`, `docs/plugins-publishing.md` | author guide moves to the SDK repo; host limits stay as an internals page |
| `wit/format.wit` (visual-editor adapters) | stays until adapters become third-party |

The Rust and the Node validators duplicate the archive and permission rules today
(`archive.rs` versus `archive.mjs`): one implementation must win (the CLI, run inside
registry CI), with shared vectors until then.

## What must change in Manang

* `PluginMeta` (`crates/manang-kernel/src/manifest.rs`) gains optional `license`,
  `homepage` and `api` (all `#[serde(default)]`; today `license` and `homepage` are
  silently ignored although the registry and the publishing guide use them).
* An `api` check at install (`crates/manang-plugins/src/install.rs`) and at load
  (`crates/manang-wasm-host/src/runtime.rs`), replacing the fragile text match on
  "manang:plugin/handler"; clear user-readable errors in Problems.
* The test fixtures that need `sdk/` become committed prebuilt components plus a contract
  job in the SDK repo.
* Must not break: first-party native plugins, existing WASM installs (no `api` field),
  the `.manangplugin` layout and limits, the registry client and signed index, offline builds,
  and the AGENTS.md invariants (no new C or system libraries in the app; `wasmtime` and
  `wit-component` only inside the CLI).

## Work packages (at most one heavy build at a time)

| # | Package | Size | Needs the owner |
| :--- | :--- | :--- | :--- |
| P0 | Fix the doc errors the audit found: broken review-criteria pointers, stale CHANGELOG key line, stale ops text | S, docs only | no |
| P1 | ADR 0017 "plugin SDK extraction" recording the decisions above | S, docs only | confirms |
| P2 | Manifest `license`, `homepage`, `api` plus install and load checks, with tests | S/M, light (`-p manang-kernel -p manang-plugins`) | no |
| P3 | `manang-sdk` skeleton: move WIT, componentize, example, template (history via subtree split) | M | repo creation |
| P4 | `manang-plugin` crate, macros, `manang-plugin-test` fake host | L, heavy, inside the SDK repo | no |
| P5 | `cargo-manang`: new, build, pack, validate; then dev, test, release, publish | L, heavy | no |
| P6 | Registry repo extraction (keep the old one until the new CI is green; the signed index must not be interrupted) | M, Node | repo, secrets |
| P7 | Host vendoring: `SDK_PIN`, sha test, fixtures, delete `sdk/` and the script | M, heavy, run alone with the full gates | no |
| P8 | GitHub Action, template repo, prebuilt CLI binaries | M | template flag |
| P9 | Docs: split, quick start, guides, API changelog, policies, WIT reference generated from the WIT | M, docs only | contact address |
| P10 | Docs site wiring in `fundaments-landing` | S | secret |
| P11 | Publish crates, tag `wit-v0.2.0` (dry run first) | S | crates.io token |
| P12 | Revocation list (signed) plus client check; automated wasm import checks fail closed | L, heavy | no |
| P13 | Host support for two interface versions | L, only when a second exists | no |

Order: P0, P1, P2 (with P3 as moves), P4, P5, P6 in parallel with P4/P5 (Node only), P7
alone, P8, P9, P10, P11, then P12 before any public launch.

## Owner needs (nothing is created without a go)

Create the three repositories in the organisation and mark the template repo as a template;
a crates.io account and a scoped token; the registry signing key moved as a secret into the
registry repo (never regenerated: a new key forces an app release); `STUDIO_SYNC_TOKEN` for
docs sync; a security and takedown contact; reviewers and the review-time target; a real
registry hostname decision (the plan still says `registry.manang.invalid`).

## Risks

Pre-1.0 component interfaces are matched by exact version, so each minor is a break until
the multi-version host exists. No async in 1.x (the host is synchronous and stateless per
call). Wasmtime skew between the CLI and the app (pin both). Name squatting and namespace
attacks on the registry. Two validators drifting. The CI-minutes budget (path-filter the SDK
workflows; contract job manual or weekly). The interface has already changed once (0.1 to
0.2): authors in the first months will take breaking changes.
