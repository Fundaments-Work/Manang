# ADR 0014: Web version and accounts

Status: **draft, proposed**, 2026-10-18. Nothing is decided until the owner
accepts it. Full reasoning, cost model and sources:
`docs/plans/2026-10-18-web-version.md`. Related: ADR 0003 (WASM plugins),
ADR 0013 (live editing and the ciphertext relay), AGENTS.md invariants 5 and 6.

## Context

The owner wants a web version so people can try Manang without installing
anything, hosted on Cloudflare, with the backend, accounts and sync written in
Rust and light on memory and CPU. The project is local-first (plain UTF-8
files, git) and has no Manang-operated service except the optional
ciphertext-only relay of ADR 0013. A web version needs a hosted origin and, for
cloud sync, accounts. Those are exceptions to the local-first stance and must be
bounded.

Verified facts (documentation read 2026-10-18; see the plan for URLs):
Cloudflare Workers static assets and Pages share limits (20,000 files free,
25 MiB per file) and Cloudflare says Workers has "a broader set of features";
Rust Workers (`workers-rs`) build with `wasm-bindgen` and support Durable
Objects, D1 (labelled alpha in the README), R2, KV and Queues, but not tokio, and
by default abort the instance on panic; Free-plan CPU is 10 ms per request;
Email Sending is Beta and needs Workers Paid; WebAuthn PRF support varies by
browser and authenticator; OPFS is widely available while
`showDirectoryPicker` is not.

## Decision (proposed)

1. **Hosting.** One Cloudflare Worker with static assets serves the app and the
   Rust API and holds the Durable Object, D1, R2 and Queue bindings. Pages is
   acceptable for a static-only demo. The anonymous live relay stays a
   separate Worker with its own wire contract (ADR 0013).
2. **Split of work.** The browser runs the application: the Rust kernel and
   web-capable plugins compiled to `wasm32-unknown-unknown` in a dedicated Web
   Worker, behind a `wasmKernel` implementation of the existing `Kernel`
   client. The edge only authenticates, stores ciphertext, orders and forwards
   frames, and sends mail. It never decodes documents, never runs a CRDT and
   does not render. OS access moves behind traits (`ProjectFs`, HTTP client,
   `SyncTransport`).
3. **Files.** The default project store is OPFS; folders on disk through the
   File System Access API where available; zip import/export always. No git in
   the first release. A cloud project is a ciphertext replica, never the only
   copy the app encourages.
4. **First release scope.** Typst and Markdown, figures, search, writing tools,
   a bring-your-own-key agent, anonymous live rooms. LaTeX is desktop only until
   a browser engine (the Tectonic WASI module first, per the zero-install LaTeX
   research) is proven. Third-party plugins are not offered on the web;
   first-party plugins are compiled in. A future web plugin tier needs its own
   ADR (Worker isolation, separate-origin sandboxed UI).
5. **Single-threaded first, no cross-origin isolation.** Avoid
   `SharedArrayBuffer` until measurements force it.
6. **Accounts are optional** and exist only for cloud sync and account-backed
   sharing. Sign-in: passkeys first, email magic link as fallback and recovery,
   ORCID/GitHub/Google OIDC later. No passwords, no SMS. Sessions are opaque
   random tokens, hashed at rest, in `__Host-` `HttpOnly` `SameSite=Lax`
   cookies; same-origin API; Origin checks; Turnstile and rate limits on
   registration and mail endpoints.
7. **Auth implementation.** A deliberately small own implementation in a Rust
   Worker (RustCrypto primitives, hand-written assertion verification, fuzzed
   parsers) with an external security review before sync ships. Fallback: a
   hosted identity provider for login only, with encryption keys independent of
   it.
8. **End-to-end encrypted sync is the default design.** A random project key
   per project, wrapped under a user-held account key; the account key comes
   from the passkey PRF where supported, a recovery key shown once, or a
   passphrase (Argon2id). The server stores only wrapped keys and ciphertext.
   Losing every passkey and the recovery key makes cloud data unrecoverable; the
   product states this before enabling sync. A "server can read" tier is an
   owner option, to be labelled truthfully, not a default.
9. **Storage roles.** D1: accounts, credentials, sessions, project index,
   quotas. Durable Object (SQLite) per project: ordered encrypted frame log,
   snapshot pointer, WebSocket fan-out with Hibernation. R2: content-addressed
   ciphertext blobs and snapshots. Queues: erase, export, mail retries. KV:
   nothing security-relevant. Compaction is client driven.
10. **Privacy posture.** Store the minimum (id, email, credential public keys,
    session hashes, quota counters); never document plaintext by default, keys,
    passphrases, AI provider keys or IPs beyond a rate-limit window. No
    analytics by default; aggregate counters only; opt-in crash reports.
    GDPR export and erase are built before launch.
11. **Browser hardening.** Strict CSP (`script-src 'self' 'wasm-unsafe-eval'`,
    `connect-src` allow-list, no inline script), HSTS, nosniff, no referrer,
    Trusted Types for HTML sinks, self-hosted fonts and scripts, hash-pinned
    wasm, locked dependencies and `cargo deny`.

## Consequences

* Positive: a try-without-install path that reuses the existing frontend and
  the Typst engine; a tiny, auditable server; a privacy claim that can be
  verified (the server holds ciphertext); one codebase of logic; infrastructure
  cost estimated in the tens of dollars a month at 10,000 monthly active users
  (assumption-driven; see the plan).
* Negative: a new operated service (availability, abuse, legal, support); a
  refactor to put the project plugin behind `ProjectFs` and to gate native
  dependencies for wasm; unrecoverable data if keys are lost; browser storage
  can be evicted; Rust Workers are young; the AGENTS.md performance budgets were
  set for native and must be re-measured on wasm; security-critical own auth
  needs a paid review; live rooms that never hibernate cost Durable Object
  duration, which ADR 0013's sizing did not count.
* Amends ADR 0013's cost note: a chatty room is billed for wall-clock duration
  (128 MB while active), not only requests. Per the plan, about 920 GB-s per
  two-hour session.

## Open points (owner)

Hosting choice; web v1 scope; own auth versus hosted IdP; E2EE-only versus an
optional server-readable tier; email sender; AI proxy or none; plans and
quotas; legal (EU location, DPA, terms for ciphertext hosting, takedown); budget
for the security review; Loro versus yrs for the web. Phase 0 spikes (wasm32
audit, Typst in a Worker, PRF matrix, COOP/COEP via `_headers`) must pass before
this ADR is accepted.
