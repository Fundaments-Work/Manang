# Releasing the built-in LaTeX engine

For the lead. The optional engine is the `manang-latex-worker` sidecar (Tectonic
inside). The app never links it; users download it once, with consent, from a
host you choose. This page is how you build it, hash it, host it and ship the
numbers. Design: ADR 0004 (amendments), `crates/manang-latex/src/builtin.rs`.

## How the pieces fit

| Piece | Where | Trusted? |
| :--- | :--- | :--- |
| Release manifest | `engines/manifest.json`, embedded into the app at build time | **Yes. The only trust root.** |
| Base URL | build-time constant `MANANG_ENGINE_BASE_URL` | No. Only says where to fetch from. |
| The binary | `<base URL>/tectonic-<version>/<file>` | Only if its size and SHA-256 equal the embedded manifest. |

The app downloads `<base>/tectonic-<version>/<file>` (so with the default
manifest: `<base>/tectonic-0.17.0/manang-latex-worker-linux-x86_64`), refuses
anything whose size differs or whose SHA-256 differs from the embedded entry,
runs `<file> --version` once, and only then renames it into
`<app data>/engines/tectonic-<version>/`. A compromised host can serve bad
bytes; it cannot get them installed. The base URL must be `https` (loopback
`http` is allowed for tests).

The default base URL is the placeholder `https://engines.manang.invalid/engines`.
Until a build sets a real one, `latex.engine-status` reports
`downloadable: false, reason: engine_not_published` and the UI shows no download
button (Settings > LaTeX says the engine is not available in this version yet).

## 1. Build

CI (preferred; it is the only build that checks portability):

1. Actions > **release-engine** > Run workflow (or push a tag `engine-v0.17.0`).
2. It builds on `ubuntu-24.04`, statically linking ICU, FreeType and friends
   (`PKG_CONFIG_ALL_STATIC=1`), fails if the result needs any dynamic library
   other than the C library and compiler runtime, runs `--version`, compiles
   `testdata/corpus/latex-article` with it, and prints the manifest lines in the
   job summary.
3. Download the artifact `engine-linux-x86_64` (the binary and its `.sha256`).

**Important, measured 2026-10-18:** a plain `cargo build` on Arch produced a
14,413,816-byte binary that links these system libraries dynamically:
`libicuuc.so.78`, `libicudata.so.78`, `libfontconfig.so.1`, `libfreetype.so.6`,
`libgraphite2.so.3`, `libpng16.so.16`, `libexpat.so.1`, `libbz2.so.1.0`,
`libbrotlidec.so.1`, `libbrotlicommon.so.1`, `libz.so.1`, `libstdc++.so.6`,
`libgcc_s.so.1`, `libm.so.6`, `libc.so.6` (HarfBuzz is already static). The ICU
version is part of the soname, so that binary only runs on a machine with ICU 78.
**Do not publish a locally built binary.** The workflow's static link step is
written but has not been run (no static `.a` files on the dev machine). If it
fails, the fallbacks are, in order: build inside a `manylinux`/old-Ubuntu
container with the same static flags; or ship a `.tar.zst` containing the worker
plus its `.so` files with `RPATH=$ORIGIN` (this needs a small installer change:
unpack instead of rename). The installer already refuses to keep a binary that
cannot start (`--version` smoke test, error `engine_wont_run`), so a bad
publish fails safe at the user's machine, not silently.

Local build (for measuring only; use the HDD target dir, never a second target on
the SSD, and nothing else heavy running):

```bash
CARGO_TARGET_DIR=/run/media/sapiens/Development/Fundaments/manang-release-target CARGO_BUILD_JOBS=4 \
  cargo build --release --locked -p manang-latex --no-default-features --features engine \
  --bin manang-latex-worker
```

`--no-default-features` leaves out the app's HTTP client (`download` feature) so
the sidecar carries no extra network stack. About 3.5 minutes cold, 2 minutes
when only `manang-latex` changed (measured on a loaded machine).

## 2. Hash

```bash
sha256sum manang-latex-worker-linux-x86_64     # or take the .sha256 artifact
wc -c < manang-latex-worker-linux-x86_64       # size in bytes
```

## 3. Host

Upload the file to the host you choose, at exactly:

```
<base URL>/tectonic-<version>/<file name from the manifest>
```

Requirements: HTTPS, `Range` requests supported (downloads resume; a server that
ignores `Range` still works, it just restarts), no required headers or cookies,
no login, no redirects needed (redirects are followed, up to ten). Any static
host or object store works; a CDN is fine because the checksum, not the host,
is trusted. Do not upload from here; the host decision is yours.

Nothing here uses Cloudflare or GitHub credentials. (If you pick GitHub Releases
or R2, the URL shape above still has to hold, or change `EnginePlan::url` in
`builtin.rs`.)

## 4. Publish

1. Edit `engines/manifest.json`: set the platform entry to the printed lines
   (`status`, `file`, `size`, `sha256`). **The checked-in linux-x86_64 numbers are
   the 2026-10-18 local Arch build and must be replaced by the CI build's.**
2. Build the app with the base URL baked in:
   `MANANG_ENGINE_BASE_URL=https://<host>/<path> pnpm app:build`
   (it is read with `option_env!` in `crates/manang-latex/src/builtin.rs`).
3. Check on a clean machine without TeX: preview > Download the built-in engine >
   consent window shows your host, size and hash > it installs > the document
   compiles. Then `latex.engine-status` shows `installed: true`.
4. Cut the app release as usual.

## Shipping a new engine version

The manifest is compiled into the app, so **a new engine version ships with a
new app release**: bump `version` and the platform entries in
`engines/manifest.json`, upload the files under the new
`tectonic-<version>/` path (keep the old path up while old app versions exist),
release the app. On first launch of the new app the old engine is reported not
installed (its folder is `tectonic-<old version>`); installing the new one
deletes older `tectonic-*` folders. If a hot fix must ship without an app
release, the follow-up is a signed manifest fetched at runtime (an ed25519
signature over the manifest, public key embedded in the app); not built.

## What needs the network, and where things live

First compile on a machine with the engine installed and `latex.fetch-packages`
on (the app default):

1. **The package bundle index** from `relay.fullyjustified.net`
   (`default_bundle_v33.tar.index.gz` and its hash file). That host redirects to
   `data1b.fullyjustified.net`, which serves the 2.9 GB bundle tar by HTTP range
   requests. The whole tar is never downloaded.
2. **Each file the document needs**, as a ranged read. An article plus a report
   with a bibliography cost about 20 MB; the cache after the corpus's LaTeX
   projects was 43 MB.
3. Nothing for the format file (`latex.fmt`, about 24 MB): the worker generates
   it locally on the first compile.

Cold first compile measured here: dominated by the network, from 1 to 3 minutes
for the corpus article on a busy machine and connection. Warm compile: well under
a second (`1.1 s` for three projects including process start).

Cache layout (`LatexConfig::cache_dir`; in the app `<app cache dir>/latex`, on
Linux `~/.cache/work.fundaments.manang/latex`; override with `MANANG_LATEX_CACHE`):

```
bundles/data/<hash>, <hash>.index     downloaded package files
bundles/hashes/…default_bundle_v33.*  which bundle version the cache holds
formats/                               generated format files (latex.fmt)
builds/<project hash>-<engine>/        system-TeX build folders (not used by Tectonic)
```

Pre-seed for offline use: compile the documents once on a connected machine
with the same engine version, then copy `bundles/` and `formats/` into the same
cache directory on the offline machine, and turn off Settings > LaTeX >
**Fetch LaTeX packages on demand** so the worker never tries the network.
Verified: with a warm cache and the network pointed at a closed port, the
corpus article compiles; with an empty cache the compile fails with
`bundle_unavailable` ("this bundle isn't cached"). The cache is keyed by the
bundle URL; the format files by the engine version, so seed with the same
version. A package no compile ever needed is not in the cache: seeding means
compiling a document that uses it. A directory or `.ttb` bundle can replace the
remote one through `LatexConfig::bundle` (no UI for that yet).

## Per-platform status

| Platform | Manifest | Built | Notes |
| :--- | :--- | :--- | :--- |
| linux-x86_64 | built | local only | CI static build untested; see above |
| linux-aarch64 | planned | no | enable the arm runner in the workflow |
| windows-x86_64 | planned | no | vcpkg static triplet; slow ICU build |
| macos-aarch64 | planned | no | needs signing and notarisation |
| macos-x86_64 | planned | no | same |

A `planned` entry has no `size` or `sha256`; the app treats the platform as
unsupported and offers no button.

## Licences

Tectonic is MIT; the XeTeX engine, ICU (ICU licence), FreeType (FTL/GPLv2),
HarfBuzz (MIT) and Graphite2 (LGPL/MPL) are linked into the worker. Ship the
notices with the host page (or inside a `.tar` with a `THIRD_PARTY_NOTICES`
file) and keep the build recipe public; the static LGPL parts need the relink
information. This is an inference, not legal advice; review before general
availability.
