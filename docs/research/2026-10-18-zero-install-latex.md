# Research: compiling LaTeX with nothing installed

Date: 2026-10-18. Sources: GitHub API and raw files fetched on 2026-10-18,
`pkg.go.dev`, TinyTeX FAQ. Two small experiments were run in a scratch
directory (section 3). Licence readings are inferences, not legal advice.
Follows `2026-10-18-tex-fallback.md`; this note adds verified facts and the
Wasmtime question.

## Verdict

Zero-install, offline-after-download, sandboxed LaTeX is achievable, but only
one route fits our host cheaply: **Tectonic built as a WASI module**
(`wasm32-wasip1`, 5.2 MB, 22 plain WASI imports). The Emscripten-based
engines (LibrePaper, BusyTeX, TeXlyre) are browser-first, carry roughly 76
Emscripten-specific imports, and would need a purpose-built shim to run in
Wasmtime. For the beta in a few weeks, ship the existing Tectonic worker as a
consented download; it is the only option that is already built.

## 1. LibrePaper `wasm-latex`

Facts (repo files read 2026-10-18; API data):

* MIT for the repo's own code; created 2026-09-09, last push 2026-10-17, 41+
  commits, 1 star, no tagged releases, 0 open issues. Very new and
  single-vendor, but moving daily.
* Target: **browser only**. Emscripten 3.1.46 for the engines, Web Workers,
  synchronous XHR for bundle fetches. WASI is not mentioned anywhere.
* Engines: pdfTeX (with SyncTeX), XeTeX plus xdvipdfmx (SyncTeX not yet),
  dvipdfm-x, BibTeX, BibTeX8, makeindex, Biber 2.22 (Perl compiled to WASM,
  Emscripten 5.0.4), experimental LaTeXML. LuaTeX: Dockerfile exists, but
  `what-works-in-the-browser.md` says the build has "an undiagnosed timeout".
* Packages: TeX Live snapshot `texlive-20260301`. The tree is packed into
  5,479 deterministic tar bundles (73 split parts), 159,000 files, **3.49 GB
  in total** (fonts 2.77 GB, macros 645 MB). A pre-merged `core` bundle is
  18.3 MB. A worker fetches a whole bundle on the first request for any file
  in it, verifies its SHA-256, and unpacks it into a virtual filesystem.
  Bundles are content-addressed and `immutable`. A plain article needs about
  5 requests cold, 0 warm; TikZ or beamer about 20. The index doubles as an
  existence check, so a missing package fails in under a second without any
  request. Formats (`.fmt`) are prebuilt, with the clock frozen at
  `SOURCE_DATE_EPOCH`.
* Not covered: shell escape in every form (minted, TikZ externalize, gnuplot,
  epstopdf, svg) is declared to need the "paired app". No speed or memory
  numbers are published. Multi-pass builds are not described beyond Biber
  caching.
* Licences (`linked-components.json`, `THIRD_PARTY_NOTICES.md`, `RELINK.md`):
  everything is **statically linked** into each engine. pdfTeX and XeTeX are
  GPL-2.0-only combined, dvipdfm-x GPL-2.0-or-later, BibTeX8 GPL; kpathsea,
  Graphite2 and TECkit are LGPL-2.1+, with a stated relink obligation (ship
  the full build recipe and source). FreeType is taken under GPL-2.0, Xpdf 4.06
  under GPL-2. **Biber is AGPL-3.0** because the vendored build is TeXlyre's;
  the notices file offers no replacement and says each release must ship
  corresponding source. LPPL covers most TeX Live packages.

Meaning for us: the engines are GPL. An MIT app can download and run them as
separate artefacts, but must not statically combine them with app code, and
must carry notices and a source offer. To avoid AGPL Biber, ship only
BibTeX/BibTeX8 (natbib, `backend=bibtex`) or use native `biber` when present.

## 2. Comparison of the other options

| Option | Version / date (verified) | Licence | Download | Engines | Biber | Shell escape | Health |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Tectonic native (our worker) | 0.17.0, 2026-07-27; repo push 2026-08-01; 173 open issues | MIT (LICENSE file; the API says NOASSERTION) | CLI tarball 9.9-22.7 MB per platform, plus bundle | XeTeX only | external `biber` only | off | maintained, slow cadence |
| Tectonic as WASI (`mgilbir/tectonic@wasm`, used by `tecgonic` v0.2.1) | module 5.2 MB; fork push 2026-07-16; tecgonic published 2026-09-04 | MIT (tecgonic); Tectonic MIT | module plus bundle dir | XeTeX only | none | omitted by design | one maintainer, 0 stars |
| TeXlyre BusyTeX (`texlyre-busytex`) | assets-v1.4.1, 2026-10-16; TeX Live 2026 | **AGPL-3.0** | 521 MB tarball; WASM 32.5 MB; data packages 93, 201, 342 MB; Biber 9.6 MB wasm plus 21 MB data | pdfTeX, XeTeX, LuaHBTeX | yes (own wasm) | `shell_escape_dispatch_js` hook | active, 36 stars |
| BusyTeX upstream | push 2026-10-16; TeX Live **2023** | scripts MIT; the repo has no licence field; binaries carry TeX Live licences | n/a | pdf, Xe, Lua | not ported | not mentioned | active, 77 stars |
| SwiftLaTeX | last push **2024-06-18** | AGPL-3.0 | on-demand package server | pdf, Xe | no | no | dormant |
| `texlive.js` | last push **2017-01-24** | GPL-2.0 | n/a | pdf | n/a | n/a | dead |
| TinyTeX (TeX Live scheme) | v2026.10, 2026-10-01 | the repo's licence is MIT (R package) but the distribution is TeX Live (GPL/LPPL); the FAQ states no licence and says redistribution was cleared with TeX Live developers | TinyTeX-1: 53.6 MB (Linux x64), 67.2 MB (macOS), 73.7 MB (Windows exe); TinyTeX-0: about 1.4 MB to 24 MB; TinyTeX (default): 152-211 MB | pdf, Xe, Lua | yes | yes (restricted by default) | well maintained |
| TeXres | created 2026-09-16 | NOASSERTION | claimed 800 MB to 1.4 GB | claimed all | claimed | n/a | 1 month old |

* **TeX Live parity**: only TeX Live itself (TinyTeX, system TeX) has full
  parity. Tectonic lacks pdfTeX/LuaTeX specifics and shell escape. BusyTeX-style
  builds track TeX Live macros but lack shell-escape tools.
* **Speed**: only one number exists for the WASI route: tecgonic reports a
  40-row `longtblr` document at 17.4 s on an M1 Pro (9.0 s with one pass) under
  wazero, which is a heavy stress test with no native baseline. Tectonic native
  measured in our own ADR 0004: 0.44 s warm, 2.5 s first compile (format
  generation). We have no WASM-versus-native comparison; treat 2 to 5 times
  slower as an assumption to test.
* **TinyTeX** needs `tlmgr` for extra packages. A missing package triggers an online install, so
  "offline after first use" holds only for packages already pulled. It is
  per-platform (a macOS copy does not run on Windows). It is a native process,
  so sandboxing is whatever the OS gives us.

## 3. Running a WASM TeX in our Wasmtime host

Our host (ADR 0003, `crates/manang-wasm-host`, Wasmtime 49 with the
component model, pooling allocator, epoch interruption, per-call fresh `Store`
with a memory cap, `.cwasm` cache) loads **components** against our WIT
worlds, with **no WASI**. A TeX engine is a core module that expects a
POSIX-like filesystem. So it does not fit the plugin tier as is; it needs a
separate "engine" path in the same crate (core module, own imports, same
limits and epoch machinery).

Experiments run (scratch directory only, nothing installed, nothing in the
repo). Wasmtime and `wasm-tools` are not installed here, so Node's WebAssembly
and `node:wasi` were used for inspection:

1. `busytex.wasm` from TeXlyre `assets-v1.4.1` (32.5 MB): **85 imports**, 76
   from module `env` and 9 from `wasi_snapshot_preview1`. The `env` imports
   are Emscripten's: dozens of `__syscall_*` entries (openat, stat64,
   getdents64, pipe2, poll, mmap...), about 20 `invoke_*` trampolines (JS-side
   setjmp/longjmp and exception emulation), time functions, and two
   project-specific JS callbacks, `kpse_remote_fetch_js` and
   `shell_escape_dispatch_js`. `biber.wasm`: 89 imports, with `js_eval_js` and
   `sigsuspend`. Under plain Wasmtime these do not link; a Rust implementation
   of the Emscripten syscall ABI, the `invoke_*` trampolines (catch the
   longjmp trap, call exported `setThrew`, restore the stack) and a VFS would
   have to be written. The last part is the uncertain one; I did not try it.
2. `tectonic_wasi.wasm` from `tecgonic` (5.2 MB): **22 imports, all
   `wasi_snapshot_preview1`** (path_open, fd_read, fd_pread, fd_readdir,
   random_get, clock_time_get, proc_exit, and so on). Exports `_start`,
   `tectonic_abi_version`, `tectonic_compile`, `tectonic_compile_defaults`,
   `tectonic_generate_format`, `malloc`, `free`, `memory`. Instantiated under
   Node's WASI host: `tectonic_abi_version()` returned 2, initial memory 83
   pages. I did **not** run a compile (the call convention is not documented
   in what I read, and a bundle directory is needed) and did **not** run it
   under Wasmtime. The module is a standard reactor, so I expect it to link
   with `wasmtime-wasi` preview1, or with the 22 functions hand-written onto a
   core `Linker`. That is an expectation, not a result.

What the host would need for the WASI route:

* A **preview1 surface restricted by capability**: preopen two directories,
  the read-only bundle directory and a per-compile temp directory (input
  copied in, PDF/log/SyncTeX read out). Hand-writing the 22 imports on top of
  `cap-std` (already used by our `fs` interface) preserves the "no ambient
  syscalls" invariant and avoids a new crate; using `wasmtime-wasi` is faster
  to write but adds a dependency and a broader surface. Decision needed.
* Limits: the existing `Store` limiter (raise the 64 MiB plugin default to
  about 512 MiB for the engine), the epoch deadline (a looping document is
  killed; tecgonic reports 5x slowdown for its cancellation, ours uses epochs
  and is expected to be cheaper, to be measured), and a disk quota on the temp
  dir (neither Wasmtime nor tecgonic bounds it).
* Run it on Rayon (it blocks), one instance per compile, `.cwasm` precompiled
  and cached (tecgonic measures 1.4 s uncached versus 50 ms cached start).
* A clock: pass a fixed `SOURCE_DATE_EPOCH`; the fork honours it (ABI 2), so
  output is reproducible.
* Bundle: the engine expects an **extracted bundle directory** (plus a fonts
  dir and a generated `latex.fmt`), not a `.ttb` served over HTTP. We would
  download and unpack the Tectonic bundle ourselves; its size I could not
  verify from primary sources (an earlier note cites about 800 MB; unverified).

Effort (one engineer, assuming the module behaves): engine path in the host
and driver 5 to 8 days, bundle download/verify/unpack 3 days, parity tests
against the native worker (diagnostics, SyncTeX, determinism) 3 days. Risks:
the fork has a single maintainer and falls behind upstream until someone
rebases (its own docs say so); performance unknown; ABI drift; XeTeX only.
Emscripten route: 4 to 8 weeks (syscall and `invoke_*` emulation, VFS), high
uncertainty.

## 4. Recommendation

### (a) Beta in a few weeks: Tectonic worker as a consented download

State today: `manang-latex-worker` exists behind the `engine` feature; it is
not bundled; the offline path is "pre-seeded cache" (ADR 0004). The upstream
`tectonic` release assets (9.9 MB musl Linux, 21.1 MB Windows MSVC, 21.7 MB
macOS arm64) are the CLI, not our worker, so they are not a drop-in. Missing:

1. **Build and sign our worker per OS** in CI: Windows MSVC (x64), macOS
   arm64 and x64, Linux x64. Tectonic's native dependencies (HarfBuzz, ICU,
   Graphite, fontconfig on Linux) make Windows and macOS the risky builds.
   macOS signing and notarisation of the downloaded binary needs checking.
   Effort 4 to 6 days.
2. **Download manager**: on first LaTeX compile with no TeX found, ask
   consent, fetch the worker (about 10 to 22 MB) from our release host, verify
   a pinned SHA-256 (and ideally a signature), unpack into the app data dir.
   2 days.
3. **Bundle download and cache**: fetch the Tectonic bundle on demand with
   progress, resumable, into `cache_dir`, then flip `allow_network` off; test
   a fully offline second run. Prefetch-all is a follow-up from ADR 0004. 3
   days. Ship a seeded `latex.fmt` to avoid the 2.5 s first-compile cost (1
   day).
4. **Offline mode**: surface the existing "file not found plus hint" error as
   "package X not cached; go online once". 1 day.
5. **Licence notices**: Tectonic and its bundled TeX Live code (XeTeX
   GPL-2.0-or-later family, HarfBuzz MIT, ICU, Graphite LGPL), LPPL for the
   bundle packages, a source offer for the worker, a Third Party Notices
   screen. 1 to 2 days, plus the owner's confirmation.
6. Beta tester guide: state clearly that Tectonic means XeTeX only, no
   `shell-escape`, no pdfLaTeX-only packages, one error at a time. 0.5 day.

About 2 to 3 weeks for one person. TinyTeX (53 to 74 MB for TinyTeX-1) has the
best parity but needs network `tlmgr` installs and is unsandboxed; keep it as an
"Install TeX for me" option for pdfLaTeX users.

### (b) After the beta: the WASI Tectonic module

Run `tectonic_wasi.wasm` in the host's new engine path (section 3). Reasons:
no per-OS native build, signing or HarfBuzz symbol clash (the C code is inside
the sandbox, resolving the AGENTS.md 7.4 constraint rather than working around
it), real capability sandboxing, hard timeouts and memory caps, one artefact
for all platforms and the web. Package it as a **built-in backend** of
`manang-latex` (a `Backend::Wasi` beside `system` and `worker`), not a
third-party plugin: it is trusted first-party code with a larger preopen
surface than the plugin WIT allows. Pin our own build of the fork (record the
`TECTONIC_REF`), publish it from our CI, and plan to maintain the rebase
ourselves. First gate: a one-day spike that compiles an article under Wasmtime
and times it against the native worker. If it is more than about 3 times
slower on the benchmark document, keep native as the default and WASI as the
safe fallback.

Do not adopt LibrePaper or TeXlyre for the desktop: Emscripten, browser-first,
AGPL or GPL-with-relink duties, and LibrePaper has no release yet. Re-evaluate
LibrePaper when it publishes tagged artefacts.

### (c) Web version

Browsers cannot spawn processes, so the web needs WASM. (1) The same Tectonic
WASI module in a Worker with a WASI shim and a bundle served over HTTPS and
cached in IndexedDB: one engine everywhere, XeTeX-only. (2) LibrePaper or
TeXlyre engines for pdfTeX/LuaTeX parity: AGPL or GPL obligations in the web
build and 0.5 to 3.5 GB of data. Start with (1); keep (2) as an opt-in "full
TeX Live engine" once LibrePaper has releases and the licence is decided.

## Work packages

| # | Package | Effort | Horizon |
| :--- | :--- | :--- | :--- |
| W1 | CI build, sign and publish `manang-latex-worker` for Win/mac/Linux | 4-6 d | beta |
| W2 | Consent-gated worker download with SHA-256 pin | 2 d | beta |
| W3 | Bundle download, cache, offline mode, seeded `.fmt` | 5 d | beta |
| W4 | Notices, source offer, tester wording | 2 d | beta |
| W5 | Spike: run `tectonic_wasi.wasm` in Wasmtime, time vs native | 1-2 d | after beta |
| W6 | Engine path in `manang-wasm-host` (WASI-lite on cap-std, limits, epochs, quotas) | 5-8 d | after beta |
| W7 | Our own pinned build of the WASI fork, CI, rebase routine | 3-5 d | after beta |
| W8 | Web: Worker + WASI shim + bundle hosting | 2-3 wk | web |
| W9 | Browser pdfTeX (LibrePaper) evaluation once released | 1 wk | web |

## Decisions for the owner

1. Is XeTeX-only (Tectonic) acceptable for testers without TeX, or should the
   beta offer TinyTeX for pdfLaTeX parity (larger, less sandboxed)?
2. Where do we host the worker binary and bundle (own release host versus
   Tectonic's third-party bundle mirror)? Offline-after-download depends on
   not relying on a mirror we do not control.
3. Accept the GPL-family notice and source-offer obligations for a downloaded
   engine?
4. Preview1 via `wasmtime-wasi`, or hand-written cap-std shims (stricter, more
   code)?
5. Adopt and maintain our own fork of the Tectonic WASI build, or depend on a
   third party's?
6. Web: accept AGPL if TeXlyre is chosen, or limit the web to the Tectonic
   module?

## Not verified

No Wasmtime run of any module. No compile executed. WASM-versus-native
speed. Tectonic default bundle size and format. macOS notarisation behaviour
for a downloaded worker. TinyTeX distribution licence text. Whether Tectonic
0.17's Windows build works in our CI. LibrePaper package coverage and speed
(no published numbers).
