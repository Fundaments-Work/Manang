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

## 1. Build and publish

One workflow does the whole thing: Actions > **release-engine** > Run workflow
(or push a tag `engine-v0.17.0`). It builds the worker for every platform in its
matrix, checks it, uploads it to the `tectonic-<version>` release and prints the
lines to paste into `engines/manifest.json`. Then commit the manifest and cut the
app release. Nothing is uploaded by hand.

1. The job builds with `--no-default-features --features engine` (the sidecar
   carries no HTTP stack of its own) and
   `TECTONIC_PKGCONFIG_FORCE_SEMI_STATIC=1`, so ICU, FreeType, fontconfig,
   brotli, bzip2, expat, png and z end up **inside** the binary.
2. It fails if the result needs a dynamic library other than the C library, the
   compiler runtime and `libgraphite2` (below), runs `--version`, and compiles
   `testdata/corpus/latex-article` with the real package server.
3. The `publish` job attaches `manang-latex-worker-<platform>` and its `.sha256`
   to the release tagged `tectonic-<version>`, which is where the app looks
   (`<base>/tectonic-<version>/<file>`).

**Measured 2026-10-19, both Linux architectures:** `PKG_CONFIG_ALL_STATIC=1`
alone is *not* enough. pkg-config refuses to link a library it considers a
system one statically even with that flag, so the first CI run produced a binary
that still pulled in `libicuuc.so`, `libicudata.so`, `libfontconfig.so.1`,
`libfreetype.so.6`, `libpng16.so.16`, `libexpat.so.1`, `libbz2.so.1.0`,
`libbrotlidec.so.1`, `libbrotlicommon.so.1` and `libz.so.1`. The ICU version is
part of its soname, so such a binary only runs on a machine with the ICU version
of the build machine. Tectonic's own `TECTONIC_PKGCONFIG_FORCE_SEMI_STATIC=1`
emits `static=` for every dependency it can find an archive for, and fixes it.

**Do not publish a locally built binary.** On this machine (Arch) a plain
`cargo build` produced a 14,413,816-byte binary that linked
`libicuuc.so.78`, `libicudata.so.78`, `libfontconfig.so.1`, `libfreetype.so.6`,
`libgraphite2.so.3`, `libpng16.so.16`, `libexpat.so.1`, `libbz2.so.1.0`,
`libbrotlidec.so.1`, `libbrotlicommon.so.1` and `libz.so.1` dynamically, and
Arch ships no static archives for most of them at all.

**libgraphite2 stays dynamic, on purpose.** HarfBuzz is built from the copy
vendored inside `tectonic_bridge_harfbuzz` with `HAVE_GRAPHITE2`, and Debian and
Ubuntu ship `libgraphite2-dev` without a `libgraphite2.a` for the semi-static
mode to find, so it is linked dynamically (about 200 KB, and present on any
desktop with a full font stack). The workflow's check allows exactly that one
library. The app starts the binary before it keeps it, so a machine without
graphite2 gets `engine_wont_run` and a clear message instead of a broken
install. Building a static graphite2 in the job (cmake, then drop the archive
where `pkg-config --variable=libdir graphite2` points) would close the gap; that
is the first thing to try if a tester reports it.

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

The files live on the GitHub release tagged `tectonic-<version>`, which serves
them at exactly:

```
<base URL>/tectonic-<version>/<file name from the manifest>
```

with `base URL` = `https://github.com/Fundaments-Work/Manang/releases/download`
(the value `release.yml` bakes in). GitHub redirects that to a signed
`objects.githubusercontent.com` URL, and the downloader follows up to ten
redirects, so this works. Requirements for any other host: HTTPS, `Range`
requests supported (downloads resume; a server that ignores `Range` still works,
it just restarts), no required headers or cookies, no login. A CDN is fine
because the checksum, not the host, is trusted. If you pick a host whose URL
shape differs, change `EnginePlan::url` in `builtin.rs`.

The release is marked as a **pre-release** so that GitHub's "latest release"
keeps pointing at the app; `scripts/install-linux.sh` skips releases that carry
no installer, so it never offers it to install.

## 4. Ship it in an app release

1. Copy the numbers from the workflow's job summary into
   `engines/manifest.json` (`status`, `file`, `size`, `sha256`), commit, and
   merge. The manifest is the trust root, so it changes in a reviewed commit,
   never from CI. Until this lands the app still reports `engine_not_published`
   for the platforms you have not filled in, and shows no download button for
   them.
2. Tag the app release as usual. `release.yml` sets
   `MANANG_ENGINE_BASE_URL=https://github.com/Fundaments-Work/Manang/releases/download`
   (read with `option_env!` in `crates/manang-latex/src/builtin.rs`).
3. Check on a clean machine without TeX: preview > Download the built-in engine >
   the consent window shows your host, size and hash > it installs > the document
   compiles. Then `latex.engine-status` shows `installed: true`.

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
| linux-x86_64 | built | CI (`ubuntu-24.04`) | static except libgraphite2 |
| linux-aarch64 | built | CI (`ubuntu-24.04-arm`) | free for a public repository, same steps |
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
