# Research: fallback LaTeX engine (desktop without TeX, and web)

Date: 2026-10-18. Desk research only (no downloads, nothing executed). Evidence
from project pages and the GitHub API as of 2026-10-17; many points are
unverified and are marked so. Legal points are inferences, not legal advice.

Context: desktop uses the system TeX by default (ADR 0004 amendment). A
fallback is needed for desktop users without TeX and for the future web
version.

## Candidates

| Candidate | Licence | Engines | Biber | SyncTeX | Packages | Desktop without TeX | Web | Activity |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| TinyTeX / TeX Live minimal | GPL/LPPL (sources disagree on GPL version) | pdf, Xe, Lua | yes | yes | on-demand install | good | no | maintained |
| Tectonic 0.17 | MIT (API says "Other": unverified) | XeTeX only | unverified | unverified | remote bundle (~800 MB per a third party), cached | acceptable, subprocess only | no | release 2026-07-27; 172 open issues |
| TeXlyre BusyTeX (npm `texlyre-busytex`) | **AGPL-3.0** plus TeX Live GPL/LPPL binaries | pdf, Xe, Lua (TeX Live 2026) | yes (Biber WASM on demand) | yes | data packages 90-400 MB, IndexedDB cache; ~32 MB WASM | possible | yes | active (2026-10-16) |
| BusyTeX upstream | MIT scripts only; binaries under TeX Live licences | pdf, Xe, Lua (TeX Live 2023) | no | unverified | data packages | possible | yes | active |
| SwiftLaTeX | AGPL-3.0 | pdf, Xe | no | unverified | on-demand server (self-hostable) | poor | yes | dormant since 2024-06 |
| TeXres (`leoliu0/texres`) | MIT/Apache claimed (GitHub: NOASSERTION) | pdf, Xe, Lua claimed | built in, claimed | yes, claimed | all bundled; ~800 MB executable, 1.4 GB wasm | good if claims hold | maybe | created 2026-09-16, pre-1.0, solo maintainer |

Not viable: RusTeX (HTML target), XymosTeX (learning project), go-tex (work in
progress). Hosted compile service: last resort; conflicts with local-first.

## Findings that change earlier assumptions

* The TeXlyre BusyTeX build **does** support biber (WASM, loaded on demand)
  and SyncTeX; earlier "no biber" information was stale.
* The maintained browser option is **AGPL-3.0**. Shipping it inside an
  MIT-licensed web app needs a licensing decision (AGPL-compatible web build,
  or a separately distributed component with source offer and notices).
* TeXres claims a self-contained, permissively licensed, multi-engine TeX with
  a WASM build. It is about a month old; its claims (speed, fidelity, origin of
  the code) are unverified and its artefacts are very large.

## Researcher's recommendation (not yet a decision)

* Desktop fallback: download **TinyTeX** (or a TeX Live minimal) on demand,
  with consent: real TeX Live, full engines, biber, SyncTeX, lowest risk; no
  GPL/LPPL files bundled in the app. Keep Tectonic as a subprocess-only option.
* Web: **TeXlyre BusyTeX** in a worker is the only maintained browser option
  with all three engines and biber, at the cost of AGPL, large data packages
  and unknown browser performance.
* Watch **TeXres**: could unify desktop and web under a permissive licence,
  but it is too new to be a default.

## Questions only a hands-on spike can answer

1. BusyTeX cold and warm compile times (basic article, biblatex, TikZ), peak
   memory, package download sizes.
2. BusyTeX in WebKitGTK, WebView2 and Safari/WKWebView (memory limits,
   SharedArrayBuffer and cross-origin isolation requirements).
3. Offline behaviour after first use with a self-hosted package endpoint.
4. TinyTeX download and install time, disk size, behaviour behind firewalls.
5. Tectonic licence, SyncTeX, biber, bundle self-hosting.
6. TeXres output fidelity against TeX Live on 20 real documents; load time and
   memory; shell escape behaviour; code provenance.
7. Legal review of AGPL, GPL and LPPL obligations per distribution model.

Sources: github.com/busytex/busytex, github.com/TeXlyre/texlyre-busytex,
github.com/SwiftLaTeX/SwiftLaTeX, github.com/tectonic-typesetting/tectonic
(releases/latest), pkg.go.dev/github.com/mgilbir/tecgonic,
github.com/leoliu0/texres (releases), sourceforge.net/mirror/tinytex-releases,
plus the GitHub API for activity data.
