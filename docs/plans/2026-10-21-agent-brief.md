# Brief for agents: the remaining work

Date: 2026-10-21. For AI agents and engineers picking up work after the relay and live-collaboration
milestones. The state and the reasons are in [the master plan](2026-10-20-master-plan.md); this page says
how to work and what to build next. `AGENTS.md` still governs everything and is not edited by agents.

## 1. Read first, in this order

1. `AGENTS.md` (invariants, gates, git, operational hygiene).
2. [Master plan](2026-10-20-master-plan.md), then the plan or design named in your work package.
3. For relay work: the private repo `~/Development/Fundaments/fundaments-relay` (`AGENTS.md`, `README.md`,
   `WIRE.md`, `docs/2026-10-21-load-test.md`). For plugin work: ADR 0017, `2026-10-20-plugin-sdk.md`, and the
   registry repo's `docs/review-system-design.md`.

## 2. Standing rules

* **Resources.** This machine froze twice. At most two build-heavy agents at once, none while the lead runs a
  workspace gate. Check `uptime` (1-minute load under about 4) and `free -h` (over 10 GB available) before
  starting; wrap builds in `CARGO_BUILD_JOBS=4 nice -n 10`. Fuzzing, nightly and sanitizer builds run alone.
  One headless Chromium at a time, through `scripts/ui-slot.sh`. Port 1420 may belong to another project: use
  `MANANG_ORIGIN` and a different Vite port for headless runs, and stop only what you started, by PID.
* **Branches.** One worktree and one branch per package (`agent/<topic>`), created from current local `main`
  (`git worktree add -b agent/<topic> <path> main`). Commit after each coherent step. Do not push, do not
  merge, do not trigger workflows. The report names the branch and its commits.
* **Commits.** Conventional Commits, one coherent step each, the trailer
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Check `git status` and
  `git diff --cached --stat` before every commit: unrelated edits in the tree (today
  `.github/workflows/notify-docs.yml` is the owner's) are never staged.
* **Secrets and infrastructure.** Never read private keys, tokens or `/etc/fundaments-relay/relay.env`. Never
  change DNS, tunnels, Workers or the registry signing key: the owner does that. Never SSH to the relay VM; the
  lead deploys with `deploy/deploy.sh`.
* **Plugin code never goes into this repository.** Authors keep their code in their own repositories; the
  registry holds a small entry file.
* **Privacy of the relay.** Nothing the relay logs or stores may contain addresses, room ids, query strings,
  header values or payload bytes (a test asserts it; keep it passing).
* **Docs ship with the code.** Every package updates its user or developer docs, the changelog (`Unreleased`),
  and an ADR where a decision is made. Run `python3` link check over the docs you touched.
* **Quality.** No `unwrap`/`expect`/`panic!` in non-test Rust, no new lint allows, UI follows the design system
  (square corners, weights 300 and 400, lime for one action per view).

## 3. Gates before you report

Manang crates: `cargo fmt --all -- --check`, `cargo clippy -p <crate> --all-targets --all-features --locked -- -D warnings`,
`cargo test -p <crate> --locked`, and for UI changes `pnpm run typecheck && pnpm run lint` plus the relevant
vitest files. The lead runs the full workspace gate on merge.

Relay repo: the CI uses the newest stable Rust, which has lints older toolchains lack. Lint with
`cargo +1.99.0 fmt --all -- --check`, `cargo +1.99.0 clippy --all-targets --all-features --locked -- -D warnings`,
`cargo +1.99.0 test --all --locked`, and `cargo +1.99.0 check -p relay-core -p relay-ws --target wasm32-unknown-unknown --locked`.

Report: branch, commits, what changed, test counts, anything skipped and why, and anything you could not
verify. Say "not verified" rather than guessing.

## 4. Work packages

Lanes can run in parallel (two heavy agents at most). Inside a lane the order is fixed.

### Lane P: plugin system (this repo, then the public SDK and registry repos)

| Id | Package | Done when |
| :--- | :--- | :--- |
| P1 | Manifest `license` (SPDX), `homepage`, `api` (interface version); install and load checks that refuse an incompatible `api`; a backend `bundled` flag in the plugin list so `canUninstall` in `src/lib/workbench/pluginManager.ts` stops guessing from the tier. Files: `crates/manang-plugins/src/{manifest,install,manager}.rs`, `src/lib/kernel/mockManifests.ts` | tests for each refusal; built-ins and the six core plugins still cannot be removed; UI uses the flag |
| P2 | `manang-plugin` crate and macros, `manang-plugin-test` fake host, `cargo-manang` (`new`, `build`, `pack`, `validate`, `dev`, `test`, `publish`) in the SDK repo | a new plugin is created from the template, built, packed and tested with one command each, with no clone of Manang |
| P3 | Review engine (manifest, archive, component, smoke run, policy rules; JSON and SARIF output) with fixture corpus, mutation tests, a differential test against the app host, fuzz targets and the canary in registry CI | every rule has a passing and a failing fixture; the canary fails CI when a rule is disabled |
| P4 | Registry integration: three-state scan record that fails closed, ownership claim, permission diff, nightly re-scan | a half-finished scan can never publish |
| P5 | Pinned interface copy in the app (`SDK_PIN`) with a drift test; then remove `sdk/`, `registry/`, `plugins/word-count`, `scripts/build-plugin.sh`, the registry workflow and the author docs from this repo | the app builds and tests without the removed paths; links are clean |
| P6 | Revocation: a signed list and the client check. **Required before a public marketplace launch** | a revoked plugin is refused at install and disabled at load |
| P7 | Author docs rewritten for the final tooling; rule reference generated from the catalog; docs-site wiring | a new author can publish following only the docs |

Order: P1, then P2 and P3 in parallel, then P4, P5, P6, P7.

### Lane R: relay (private repo `fundaments-relay`)

| Id | Package | Done when |
| :--- | :--- | :--- |
| R1 | Conformance runner (native target) over `conformance-vectors/`, runnable against any relay address | it passes against the local build and reports per-vector results |
| R2 | Fuzz targets for the handshake, frame header and envelope check; seed corpus kept minimal; each run capped at 60 s per target | no crash or hang; findings fixed with a regression test |
| R3 | Hostile clients during load (slowloris, oversize frames, fragmented control frames, reconnect storms while `tools/loadtest.py` runs) | delivery stays 100% for honest peers; memory stays bounded; the relay never restarts |
| R4 | Automatic update on the host: pull-based, checksum-verified, health-checked, rollback on failure, no inbound access added | a bad build is rolled back automatically in a test; the update path has no secret in the repo |
| R5 | The unexplained multi-second latency through the tunnel (see the load-test notes): reproduce from a second network, measure the tunnel hop separately, report the cause | a written cause or a measured "not the relay" with evidence |
| R6 | Per-IP behaviour behind the CDN with many addresses (header trust, ban and rate behaviour) beyond the unit tests | an integration test with many forged addresses through a loopback fronting proxy |

Load tests on the VM (`tools/loadtest.py`) are run by the lead only, on the VM itself, never through an SSH
tunnel (it makes `sshd` buffer hundreds of MB).

### Lane C: collaboration security (blocked on an external review)

Do not write identity, roles, rotation, comments or suggestions code until the owner has the cryptographer's
review of revision 2 of `docs/research/2026-10-20-collab-crypto-design.md`. What agents may do meanwhile:
prepare the review package (the design, the threat model, a test-vector list) and write failing tests for the
properties in the design. Then follow `2026-10-20-collab-program.md` (C1 to C10).

### Lane U: product backlog (pick one at a time, small branches)

Visual editor (tables, citation and cross-reference pickers, outline panel, accessibility, large documents),
authoring (engine per file, live preview, package cache, snapshots, word-level diff), Git collaboration
(conflict resolver), reference library. Section 7 of the master plan has the list; write a short plan in
`docs/plans/` before building anything over a day of work.

## 5. Not for agents (the owner decides or holds the access)

The second relay and failover (O5), the external cryptographer (O6), `SECURITY.md` wording (O7), plugin policy
(licence allow-list, review time, strictness, security contact: O3), package-registry account and registry
signing key (O4), any DNS, tunnel, Worker or domain change, pushing to GitHub, tagging a release, and checking
whether the tunnel service's own logs contain request URLs (needs the owner's go).

## 6. Suggested first assignment

Run P1 and R1 together (one Rust build each, no browser). Then P2 and R2 (fuzzing alone, so P2 waits for it or
runs without builds). Keep the lead's merge-and-gate step between rounds.
