# Master plan: relay, plugins, collaboration security and the product

Date: 2026-10-20. One document for everything in flight. It summarises and links the detailed plans;
it does not replace them. The relay is a separate private repository, so relay internals (hosting,
tuning, test plans) are kept there; this page states only what the product needs. Legend: **done**
(merged and checked), **running**, **planned**, **decided**, **open** (owner question).

## 1. Where things stand

| Area | State |
| :--- | :--- |
| Live editing: one file, whole project, join plan, presence, follow, leave warning, held-change dialog | done, merged to local `main` |
| Live-session stabilisation (waiting for host, snapshot, re-sync, trailers, guards) and hardening (fuzzed decoders, path policy, a crash found and fixed) | done |
| Old relay server removed from this repo; the app keeps `manang-relay-client` | done |
| Private relay (protocol crate, core, WebSocket codec, server): deployed and working end to end through the hosted address | done; 500 users measured on the host (see section 3), CI green on Rust 1.99 |
| Plugin SDK, template and registry repos: created, populated and public | done as a skeleton; the author tooling is not built |
| Plugin review system, crypto design revision 2, collaboration program | designed, not built |
| Backups | Manang `main` pushed on 2026-10-21; the relay repo is private on GitHub |

## 2. Standards for every package

* Security: a threat-model entry, a negative test per abuse case, hostile-peer and hostile-relay
  simulations, fuzzing of every new parser (60 s per target), no `unwrap`, no C dependencies.
* Performance and memory: a benchmark with a budget (keystroke to screen p99 under 8 ms in a live
  session), per-session and per-peer memory budgets, accounting that returns to zero, a simulated
  two-hour soak under 5% growth.
* Honesty: a security claim only for what is tested; no "secure" before an independent cryptographic
  review; unrun checks are listed as unrun.
* Process: one heavy build at a time and always capped; read-only research may run beside one build;
  commit on a branch and merge with `--no-ff`; update the CHANGELOG, ADRs and user docs in the same
  piece of work; the next free ADR number is 0019.

## 3. Relay track (private repository)

**Decided.** Rust only, separate private repository (backed up privately), no TLS in the process
(a Cloudflare Tunnel terminates it), single-threaded and memory-bounded. The hostname `relay-test` is the
relay for now. Cloudflare deployment is a **future expansion goal**; the Cloudflare Worker was **removed
from the relay repo on 2026-10-21** (no users, no deployment to protect; recoverable from git history).

| Package | Status |
| :--- | :--- |
| Core, codec, server loop, per-address limits, memory watcher, private metrics listener, control-frame rule (contract revision 3) | done, deployed |
| Conformance runner (native target first) | planned |
| Load generator (`tools/loadtest.py`) and the measured 500-user run | done on the host: 500 users, 100% delivery, 8.4 MB RSS, about 15% of a core at 4.7k deliveries/s; 48 users through the tunnel; many-address run through Cloudflare not measured (`docs/2026-10-21-load-test.md` in the relay repo) |
| Fuzzing of the handshake, frame header and envelope; hostile-client suite beyond N4 | planned |
| Automatic update on the host (pull-based, verified, health-checked, rollback) | planned |
| Metrics listener enabled on the host (loopback only) | done 2026-10-21 |
| Allocator and memory tuning from measurements | not needed so far: 8.4 MB at 500 connections |
| Rename `relay-test` to `relay` (the hostname `relay.fundaments.work` already is the app's default) | waiting for two dashboard steps by the owner, see O1 |
| A second relay for failover (a single host is a single point of failure, and a free-tier host can be reclaimed for low use) | planned, owner decision |
| Write tokens and durable rooms | planned after identity; durable rooms need a redesign first |

**Cloudflare expansion path (future).** `relay-core`, `relay-ws` and `relay-proto` are sans-IO and
checked for `wasm32`, so the decision logic can run in a Durable Object. Options: a new TypeScript
Worker written to the current contract, a Rust Worker, or Cloudflare Containers. Estimated request cost at
500 live users is about $7 a month on the paid plan; the free plan cannot carry that load. Preconditions:
a conformance run against the Worker runtime.

## 4. What the relay stores (answer to "do we store user data?")

| Question | Answer |
| :--- | :--- |
| Documents, accounts, keys, a database | none; the invite key sits after the `#` and is never sent to the relay |
| Files written by the relay | none |
| In memory while running | rooms and connections, encrypted messages awaiting delivery (dropped once delivered), bounded per-address counters for rate limits; a room is forgotten when the last person leaves |
| Logs | default level logs only start-up lines; N4's tests assert no addresses, query strings, header values, payload bytes or room ids appear; the host's journal is persistent, so whatever is logged survives restarts |
| What an observer of the relay sees | that a room exists, message sizes and timing, when people connect and leave |
| Third parties | the CDN in front terminates TLS and sees addresses, timing and sizes (not text); the host provider sees VM-level network metadata |
| Not yet verified | whether the tunnel service's own error logs on the host contain request URLs (room ids); to check |

Actions: add the third-party sentence to the user doc; set the log level and retention explicitly;
publish this table in the user guide once the unverified row is checked.

## 5. Plugin system track

Repositories, all public: `manang-sdk` (interface, componentize tool, example, guide), `manang-plugin-template`
(GitHub template), `manang-plugins-registry` (entries, schemas, validator, Worker, review docs). Plugin code
stays in the author's own repository; the registry holds a small entry file; the app's Browse tab reads the
signed index. Decisions are in ADR 0017; the review system is in the registry repo
(`docs/review-system-design.md`).

| Package | Status |
| :--- | :--- |
| Manifest `license`, `homepage`, `api`; install and load checks; a backend flag marking bundled plugins (so the UI never guesses) | planned (first) |
| `manang-plugin` crate and macros, `manang-plugin-test` fake host, `cargo-manang` (new, build, pack, validate, dev, test, publish) | planned |
| Review engine: manifest, archive, component, smoke-run, policy rules; JSON and SARIF reports; fixture corpus, mutation tests, differential test against the app host, fuzzing, canary in registry CI | planned (design done) |
| Registry integration: three-state scan record that fails closed, ownership claim, permission diff, nightly re-scan | planned |
| Pinned interface copy in the app with a drift test; then remove the originals from this repo | planned |
| Revocation (signed list plus client check) | planned, **required before a public marketplace launch** |
| Rewrite the author docs; generate the rule reference from the catalog; wire the docs site | planned |
| Publish the crates; reserve names; first real plugin as proof | planned, needs the owner's account |

Built-in plugins already cannot be uninstalled in the backend or the UI; six core plugins cannot even be
switched off.

## 6. Collaboration security program (summary)

Identity, roles, key rotation, durable rooms, comments, suggestions, version history, figure co-editing,
and chunked first sync. The design (`docs/research/2026-10-20-collab-crypto-design.md`, revision 2 after a
hostile review) is owner-only linear log, original signed updates only, pinned cutoffs, key commitments,
join-only invite caps, annotations outside the CRDT, durable rooms deferred. Required before code: a
cryptographer's review of revision 2; before any marketing of roles or removal: an external review of the
identity, roles, rotation and token code. Plan: `2026-10-20-collab-program.md`.

## 7. Visual editor, authoring and the rest of the product

* Visual editor: tables, citation and cross-reference pickers, a LaTeX mode, the outline panel, accessibility,
  large documents; a decision on `.docx` (recommended: import and export only, later).
* Authoring: engine per file with multiple outputs, live-preview mode, Typst package cache, local snapshots and
  word-level diff.
* Git collaboration: a merge conflict resolver and review comments through git.
* Reference library, plugin UI, the web version.
* Production: signed installers, auto-update, translations, an accessibility pass, the organisation policy layer,
  Windows and macOS builds of the LaTeX engine, the macOS native menu bar. (`yrs` is now the opt-in
  `yrs-engine` feature of `manang-live`, out of the app build.)

## 8. Order of work

1. Done: push, load generator, the measured 500-user run. Now: the two dashboard steps for the hostname (O1).
2. Plugin manifest and install checks, then the author tooling and the review engine.
3. Relay conformance runner, fuzzing, hostile-plus-load suite, automatic update.
4. Revocation and the registry integration; the pinned interface copy; docs rewrite; first published plugin.
5. Cryptographic review, then identity and roles, rotation, comments, suggestions.
6. Visual editor and the rest.

## 9. Open decisions and what is needed from the owner

| # | Item |
| :--- | :--- |
| O1 | **Decided 2026-10-21: remove the Worker, serve `relay.fundaments.work` from the VM relay** (the app default stays `wss://relay.fundaments.work`). Needs the owner, in the Cloudflare dashboard: (1) delete the Worker `fundaments-relay` and its custom domain or route on `relay.fundaments.work`; (2) add a public hostname `relay.fundaments.work` to the existing tunnel, service `http://localhost:8787`. `relay-test` keeps working until then and can be removed afterwards |
| O2 | ~~Push Manang `main`~~ Done 2026-10-21 |
| O3 | Plugin policy: closed source (recommended: open source required), licence allow-list, review time target, launch strictness, a security and takedown contact, the registry hostname |
| O4 | Accounts: a package-registry account and token for publishing the SDK crates; signing-key custody for the registry |
| O5 | A second relay for failover, and the hosting plan for the free tier |
| O6 | An external cryptographer for revision 2 of the crypto design |
| O7 | Whether `SECURITY.md` should describe the live-session hardening and the path policy |

## 10. Risks

A single relay host is a single point of failure. A removed collaborator keeps what they saw and a viewer who
holds the room key can read: the product must say so. Cryptographic design errors are the main risk of the
collaboration program, hence design first, adversarial review and an external audit. Revocation is not built:
until it is, a takedown stops only new plugin installs. The interface is 0.x, so early plugin authors will take
breaking changes. The VM is one host: a second relay is undecided (O5).
