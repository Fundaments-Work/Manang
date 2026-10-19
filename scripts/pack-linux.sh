#!/usr/bin/env bash
# Turn the Linux artifacts of a `tauri build` into the ones we want to ship.
#
# Run inside a directory holding the built installers, for example the
# `assets/` directory of the publish job in .github/workflows/release.yml:
#
#   scripts/pack-linux.sh
#
# It does two things:
#
#  1. Deletes the AppImage. Tauri builds it with linuxdeploy, which follows
#     `ldd` and copies the whole GTK and WebKitGTK stack into it: the
#     v0.1.0-beta.1 AppImage was 120 MB, of which 91 MB was libwebkit2gtk and
#     32 MB libjavascriptcoregtk, for a 96 MB application binary. That copy is
#     a browser engine frozen at build time, so it never gets the security
#     updates the system's does, and it is three times the size of everything
#     else we ship. Tauri has no switch to leave it out (bundle > linux >
#     appimage only offers bundleMediaFramework), so we do not ship the format
#     and give the other distributions a tarball instead.
#
#  2. Writes Manang_<version>_amd64.tar.xz from the .deb, for every
#     distribution that is not Debian-like or RPM-based (Arch, Slackware,
#     Gentoo, NixOS). It carries the same files as the .deb, uses the system's
#     WebKitGTK 4.1 like the .deb and the .rpm do, and installs into ~/.local
#     without root.
#
#  3. Recompresses the .deb with xz. dpkg stores the payload gzip-compressed
#     (45.8 MB for the beta.1 binary); xz brings the same payload to 31.6 MB.
#     Measured 2026-10-19 on Manang_0.1.0_amd64.deb; zstd -9, the other
#     candidate, only reached 37.8 MB. dpkg has read xz data members since 1.19
#     (2014), so every supported distribution can install it.
#
# Needs: dpkg-deb, tar, xz. No sudo and no root: everything happens in a
# temporary directory that is removed again.
set -Eeuo pipefail

log() { printf '%s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

command -v dpkg-deb >/dev/null 2>&1 || die "dpkg-deb is required"
command -v tar >/dev/null 2>&1 || die "tar is required"
command -v xz >/dev/null 2>&1 || die "xz is required"

here="$(pwd)"
deb="$(find . -maxdepth 1 -name '*.deb' -print -quit)"
[[ -n "$deb" ]] || die "no .deb in ${here}"
deb="$(basename "$deb")"
version="$(printf '%s' "$deb" | sed -E 's/^Manang_(.*)_amd64\.deb$/\1/')"
[[ "$version" != "$deb" ]] || die "cannot read the version out of ${deb}"

work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

# ── 1. The tarball, from the .deb's own payload ──
log "Unpacking ${deb}"
dpkg-deb -x "$deb" "$work/root"
tarball="Manang_${version}_amd64.tar.xz"
log "Writing ${tarball}"
# Files owned by the CI user would land in ~/.local with a UID that means
# nothing to the person unpacking them; tar keeps root for both. Busybox tar
# has no such option, hence the second attempt.
tar -C "$work/root" --owner=0 --group=0 -cJf "$tarball" . 2>/dev/null \
    || tar -C "$work/root" -cJf "$tarball" .

# ── 2. The .deb again, compressed with xz ──
log "Recompressing ${deb} with xz"
dpkg-deb -e "$deb" "$work/control"
mkdir -p "$work/root/DEBIAN"
cp "$work/control/"* "$work/root/DEBIAN/"
# Tauri's control files are owned by root in the archive, and dpkg-deb refuses
# to build as a normal user unless the ownership is normalised first.
dpkg-deb --root-owner-group -Zxz --build "$work/root" "$work/new.deb"
mv "$work/new.deb" "$deb"

# ── 3. No AppImage ──
for appimage in ./*.AppImage; do
    [[ -e "$appimage" ]] || continue
    log "Removing $(basename "$appimage"): it bundled WebKitGTK, use ${tarball} instead"
    rm -f "$appimage"
done

log ""
log "Linux artifacts in ${here}:"
for f in ./*.deb ./*.rpm ./*.tar.xz; do
    [[ -e "$f" ]] || continue
    printf '  %-40s %8s MiB\n' "$(basename "$f")" "$(( $(stat -c %s "$f") / 1048576 ))" >&2
done