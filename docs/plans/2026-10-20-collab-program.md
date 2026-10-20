# Collaboration program: every missing capability, tested to a standard

Date: 2026-10-20. Owner direction: implement all missing collaboration capabilities and test
them rigorously for security, performance and memory. Extends `2026-10-20-live-collab-v2.md`,
`2026-10-20-moat-collab-v3-visual-v2.md` and the private relay plan. The relay is a separate,
private repository; the wire contract it serves is public.

## Scope (from the capability audit)

| # | Capability | App side | Relay side |
| :--- | :--- | :--- | :--- |
| C1 | Member identity (verified names, per-edit attribution) | keys, signed membership log, update signing | none |
| C2 | Roles: viewer, commenter, editor | accept-time enforcement of roles | write tokens (refuse frames from read-only connections) |
| C3 | Removing a member securely | epoch keys, key wrapping, rekey on removal | epoch frame kind 3, stale-epoch refusal |
| C4 | Joining or editing when everyone has left | rollback counters, snapshot and update upload | durable encrypted rooms with quotas |
| C5 | Comments (threads, resolve, mentions) | annotation model on a signed side document, anchors from CRDT cursors, git export | durable rooms carry them |
| C6 | Comments UI in Source and Visual | margin gutter, popover, role-aware | none |
| C7 | Suggestions and track changes | suggest mode, accept and reject, author colours, Visual marks | none |
| C8 | Version history, named versions | checkout and restore as a new edit, tie to git, compaction epochs | durable rooms |
| C9 | Live figure co-editing | figure scene as a CRDT map keyed by node id | none |
| C10 | Large first sync | chunk the first sync of big single files | frame cap stays 8 MiB |

The relay's own remaining work (server loop, abuse controls, memory watcher, metrics,
conformance runner, load generator, 500-user measurement, fuzzing, hostile suite, deploy,
automatic update, cut-over) comes first because C2, C3 and C4 need its wire additions.

## Order (at most one heavy build agent at a time; read-only agents may run beside it)

1. Relay: A2 server loop, then ops surface, conformance runner, load generator and the measured 500-user run, fuzzing and hostile suite, deploy, cut-over.
2. C0: threat model and cryptographic design (read-only), reviewed by an adversarial second agent before any code.
3. C1 identity and membership log, then C2 roles with relay write tokens.
4. C3 epoch keys and revocation.
5. C4 durable rooms (relay store and client rollback counters), then C10.
6. C5 annotation model, C6 comments UI, C7 suggestions.
7. C8 version history, then C9 figures.

## Definition of done for every package (all required)

| Area | Requirement |
| :--- | :--- |
| Security | A threat-model entry; a negative test for every abuse case it names; hostile-peer and hostile-relay simulations (forgery, replay, reorder, truncation, rollback, partition); libFuzzer targets for every new parser run 60 s clean; no `unwrap`, constant-time comparison and zeroized secrets where keys exist; dependency policy checked (no C, no OpenSSL, no aws-lc) |
| Adversarial review | For every cryptographic or authorization package a fresh agent, given the code and not our conclusions, tries to break it; findings are fixed or recorded before merge |
| Performance | A criterion benchmark with a budget and a regression limit: keystroke to screen p99 under 8 ms with a live session; signature check cost per update; membership-log verification linear in members; join time for a project of N files; relay forward latency p99 |
| Memory | A per-session and per-peer budget; accounting returns to zero after leave (proptest); an accelerated in-process soak (two hours of simulated editing) with under 5% RSS growth; one heap-profile run per package |
| Correctness | Convergence proptests under partitions and delays; wasm32 check for web-capable crates; snapshot tests of wire frames; two-instance kernel tests through a real relay |
| Docs | An ADR, user documentation in plain voice, the threat model updated, honest limits listed |

Honesty rules: a security claim is made only for what is tested; nothing is called secure
before an independent cryptographic review; the owner is told what was not run. An external
audit is recommended before the product advertises its collaboration security.

## Shared test infrastructure (built early, used by every package)

* An adversarial simulation harness extending `manang-live`'s simulated network: a malicious
  member, a malicious relay (drop, delay, duplicate, reorder, truncate, roll back, replay old
  epochs), and partitions, with oracles for convergence and for "no unauthorized change applied".
* A performance and memory gate script (one command, capped) that runs the benches and the
  soak and compares with stored budgets.
* The relay's conformance runner, pointed at native, `wrangler dev` and production.

## Risks

Cryptographic design errors are the main risk: hence the design-first rule, the adversarial
review and the external audit recommendation. A viewer who holds the room key can read, and a
removed member keeps what they already saw: the product must say so. Durable rooms make the
relay stateful (quotas, retention, deletion, backups, rollback evidence only). Scope is large:
every package ships behind the gate or it does not ship.
