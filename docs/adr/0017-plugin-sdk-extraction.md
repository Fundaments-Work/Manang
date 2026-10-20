# ADR 0017: Plugin SDK extraction (authors never clone Manang)

Status: **proposed**, 2026-10-20. Plan: `docs/plans/2026-10-20-plugin-sdk.md`. Related: ADR 0003
(WASM plugin runtime and capabilities), ADR 0016 (plugin registry), AGENTS.md invariant 4.
Nothing in this ADR is implemented yet.

## Context

Plugins are WebAssembly components against the WIT package `manang:plugin@0.2.0`. Today an
author needs a clone of this repository: the WIT lives in `crates/manang-wasm-host/wit/`,
the componentize tool in `sdk/componentize`, the build script in `scripts/build-plugin.sh`
(which only works from inside the repository), and the template in `sdk/rust-plugin-template`.
The registry (`registry/`) has no entries, its validator duplicates the Rust archive and
permission rules in JavaScript, and the app silently ignores `license` and `homepage` in a
manifest. Obsidian and Zed show the target: a template repository, a published SDK, a dev
loop that needs no host source, automated review of every version, and a stated review time.

## Decision

1. **Repositories** under `Fundaments-Work`, all public: `manang-sdk` (the WIT as the single
   source of truth, the `manang-plugin` crate and macros, `manang-plugin-test` fake host,
   the `cargo-manang` CLI, a GitHub Action, examples, developer docs, API changelog),
   `manang-plugin-template` (GitHub template), `manang-plugins-registry` (entries, schemas,
   scripts, review guidelines, policies). The relay is a different, private project.
2. **Licences:** SDK and examples MIT OR Apache-2.0; template 0BSD; Manang stays MIT.
3. **Distribution:** publish `manang-plugin`, `manang-plugin-test` and `cargo-manang` to
   crates.io so authors never need a git dependency or a copy of our files.
4. **One source for the interface:** `manang-sdk/wit/`. Manang keeps a vendored, read-only
   copy at `crates/manang-wasm-host/wit/` pinned by `SDK_PIN` (tag and sha256 per file); a test
   fails on drift; the app build stays offline with no git dependency.
5. **Versioning:** a manifest field `api = "0.2"` under `[plugin]`, checked at install and at
   load (replacing the fragile text match on "manang:plugin/handler"). Absent means `0.2` with a
   warning, so existing installs keep loading. The host supports the current and the previous
   line; a removal needs two app releases or six months of warnings. A second interface version
   gets its own WIT directory in the host only when it exists. 0.x is declared unstable; 1.0
   freezes after about ten third-party plugins and after settings and the visual-editor format
   interface settle.
6. **Manifest:** `PluginMeta` gains optional `license`, `homepage` and `api` (all
   `#[serde(default)]`).
7. **Languages:** Rust first-class. JavaScript and Python are "preview": validate and pack work
   and are documented, nothing is promised until a hello-world builds against our pinned
   Wasmtime. No "any language" claim.
8. **Review:** automated on every version and fail closed (manifest, `api`, SPDX licence,
   archive limits, wasm imports against declared permissions, no WASI imports); humans only for
   a first submission, a newly requested capability, popularity or a flag; a published review
   time target; reserved ids with proof of repository ownership.
9. **Revocation** (a signed list the app checks) is required before a public launch. Until it
   exists a takedown only stops new installs, and the developer policy says so.
10. **Built-in plugins** are marked by the backend (`bundled`), not guessed by the UI from the
    tier, so a first-party WebAssembly plugin can never be offered an Uninstall button.

## Consequences

* Authors get a zero-clone path: template, `cargo manang new/build/validate/dev/test/publish`,
  a fake host for unit tests, a release workflow.
* One validator wins: the CLI runs inside registry CI; shared vectors until then.
* Costs: three new repositories to maintain, a published crate whose names must be reserved and
  owned by two accounts, and a host that must eventually carry two interface versions (pre-1.0
  component interfaces are matched by exact version, so each minor is a break until then).
* No async in 1.x: the host is synchronous and stateless per call.

## Not decided here (owner)

Crate names beyond `manang-sdk`, the crates.io account and token, who reviews and the review
time target, the security and takedown contact, the registry hostname, whether the
visual-editor format interface is a third-party surface, and the API freeze date.

## Rejected alternatives

* A git dependency for the WIT in the app build: breaks offline builds.
* A crates.io crate that embeds the WIT as the host's source: adds a publish step to every
  interface change; reconsider if a second consumer appears.
* Keeping the SDK in the monorepo with a published subset: authors still depend on our history
  and layout, and drift is invisible.
