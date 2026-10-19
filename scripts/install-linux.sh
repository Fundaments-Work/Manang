#!/usr/bin/env bash
# Install Manang on Linux from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/Fundaments-Work/Manang/main/scripts/install-linux.sh | bash
#
# Picks .deb on Debian-like systems, .rpm on Fedora/openSUSE-like systems and an
# AppImage everywhere else (or with --user, which needs no root). The download is
# checked against the release's SHA256SUMS.txt before anything is installed.
# Needs: curl, sha256sum. No jq, no bsdtar.
set -Eeuo pipefail

GH_REPO="Fundaments-Work/Manang"
APP_DIR="${HOME}/.local/lib/manang"
BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/128x128/apps"

log() { printf '%s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

usage() {
    cat <<'EOF'
Usage: install-linux.sh [--version X.Y.Z[-beta.N]] [--stable] [--bundle deb|rpm|appimage] [--user] [--dry-run]

  --version V     Install release tag vV (default: the newest release, betas included)
  --stable        Only consider releases that are not marked pre-release
  --bundle TYPE   deb, rpm or appimage (default: chosen from your distribution)
  --user          Install the AppImage under ~/.local without root, whatever the distribution
  --print-latest  Print the tag of the newest release and exit
  --dry-run       Show what would be downloaded and installed, change nothing
  -h, --help      This text

Remove it again with scripts/uninstall-linux.sh.
EOF
}

VERSION=""
STABLE=false
BUNDLE="auto"
USER_INSTALL=false
DRY_RUN=false
PRINT_LATEST=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || die "--version needs a value"; VERSION="${2#v}"; shift 2 ;;
        --stable) STABLE=true; shift ;;
        --bundle|-b) [[ $# -ge 2 ]] || die "--bundle needs a value"; BUNDLE="$2"; shift 2 ;;
        --user) USER_INSTALL=true; shift ;;
        --print-latest) PRINT_LATEST=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown argument: $1" ;;
    esac
done

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required (coreutils)"
[[ "$(uname -s)" == "Linux" ]] || die "this script is for Linux; see docs/user/install.md for other systems"
case "$(uname -m)" in
    x86_64|amd64) ;;
    *) die "no Linux build for $(uname -m) yet (x86-64 only); see docs/user/install.md" ;;
esac

# ── Which release ──
# MANANG_API_BASE is for testing the script against a local copy of the API.
api() { curl -fsSL -H 'Accept: application/vnd.github+json' "${MANANG_API_BASE:-https://api.github.com/repos/${GH_REPO}}/$1"; }

if [[ -n "$VERSION" ]]; then
    release_json="$(api "releases/tags/v${VERSION}")" || die "no release v${VERSION} found"
elif [[ "$STABLE" == true ]]; then
    release_json="$(api "releases/latest")" || die "no stable release published yet; drop --stable to include betas"
else
    # The first element of the list is the newest release, pre-releases included.
    release_json="$(api "releases?per_page=1")" || die "could not read the release list from GitHub"
fi

tag="$(printf '%s' "$release_json" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
[[ -n "$tag" ]] || die "no release found"
if [[ "$PRINT_LATEST" == true ]]; then
    printf '%s\n' "$tag"
    exit 0
fi

asset_url() {
    printf '%s' "$release_json" \
        | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | sed -E 's/.*"([a-z]+:[^"]*)"$/\1/' \
        | grep -E "$1" | head -n1 || true
}

# ── Which format ──
if [[ "$BUNDLE" == "auto" ]]; then
    if [[ "$USER_INSTALL" == true ]]; then BUNDLE="appimage"
    elif command -v apt-get >/dev/null 2>&1 || command -v dpkg >/dev/null 2>&1; then BUNDLE="deb"
    elif command -v dnf >/dev/null 2>&1 || command -v zypper >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; then BUNDLE="rpm"
    else BUNDLE="appimage"
    fi
fi
case "$BUNDLE" in
    deb) pattern='\.deb$' ;;
    rpm) pattern='\.rpm$' ;;
    appimage) pattern='\.AppImage$' ;;
    *) die "unknown bundle '$BUNDLE' (use deb, rpm or appimage)" ;;
esac

url="$(asset_url "$pattern")"
[[ -n "$url" ]] || die "release ${tag} has no ${BUNDLE} file; see https://github.com/${GH_REPO}/releases/tag/${tag}"
sums_url="$(asset_url 'SHA256SUMS\.txt$')"
file="${url##*/}"

log "Manang ${tag}: ${file} (${BUNDLE})"
if [[ "$DRY_RUN" == true ]]; then
    log "dry run: would download ${url}"
    exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

log "Downloading ..."
curl -fL --progress-bar -o "${work}/${file}" "$url"

if [[ -n "$sums_url" ]]; then
    curl -fsSL -o "${work}/SHA256SUMS.txt" "$sums_url"
    expected="$(grep -F "  ${file}" "${work}/SHA256SUMS.txt" | awk '{print $1}' | head -n1 || true)"
    [[ -n "$expected" ]] || expected="$(grep -F " ${file}" "${work}/SHA256SUMS.txt" | awk '{print $1}' | head -n1 || true)"
    actual="$(sha256sum "${work}/${file}" | awk '{print $1}')"
    [[ -n "$expected" ]] || die "${file} is not listed in SHA256SUMS.txt; refusing to install it"
    [[ "$expected" == "$actual" ]] || die "checksum mismatch for ${file} (expected ${expected}, got ${actual}); nothing was installed"
    log "Checksum verified."
else
    log "warning: this release has no SHA256SUMS.txt; installing without a checksum check"
fi

as_root() {
    if [[ "${EUID}" -eq 0 ]]; then "$@"
    elif command -v sudo >/dev/null 2>&1; then sudo "$@"
    else die "root is needed for the ${BUNDLE} package; run as root, install sudo, or use --user"
    fi
}

install_appimage() {
    mkdir -p "$APP_DIR" "$BIN_DIR" "$APPS_DIR" "$ICON_DIR"
    install -Dm755 "${work}/${file}" "${APP_DIR}/Manang.AppImage"
    ln -sf "${APP_DIR}/Manang.AppImage" "${BIN_DIR}/manang"
    curl -fsSL -o "${ICON_DIR}/manang.png" \
        "https://raw.githubusercontent.com/${GH_REPO}/${tag}/src-tauri/icons/128x128.png" \
        || log "note: could not fetch the icon; the launcher will use a generic one"
    cat > "${APPS_DIR}/manang.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Manang
Comment=Write papers, theses and slides in Typst, LaTeX or Markdown, and draw the figures too.
Exec=${BIN_DIR}/manang %F
Icon=manang
Terminal=false
Categories=Office;Education;Science;
StartupWMClass=Manang
DESKTOP
    update-desktop-database "$APPS_DIR" 2>/dev/null || true
    gtk-update-icon-cache -q "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
    log "Installed ${APP_DIR}/Manang.AppImage and ${BIN_DIR}/manang"
    case ":${PATH}:" in
        *":${BIN_DIR}:"*) ;;
        *) log "note: ${BIN_DIR} is not on your PATH; start Manang from the application menu or add it" ;;
    esac
    if ! ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; then
        log "note: AppImages need libfuse2 (Debian/Ubuntu: libfuse2t64). Without it run: ${BIN_DIR}/manang --appimage-extract-and-run"
    fi
}

case "$BUNDLE" in
    deb)
        if command -v apt-get >/dev/null 2>&1; then as_root apt-get install -y "${work}/${file}"
        else as_root dpkg -i "${work}/${file}"
        fi ;;
    rpm)
        if command -v dnf >/dev/null 2>&1; then as_root dnf install -y "${work}/${file}"
        elif command -v zypper >/dev/null 2>&1; then as_root zypper --non-interactive --no-gpg-checks install "${work}/${file}"
        elif command -v yum >/dev/null 2>&1; then as_root yum localinstall -y "${work}/${file}"
        else as_root rpm -Uvh --replacepkgs "${work}/${file}"
        fi ;;
    appimage) install_appimage ;;
esac

log ""
log "Manang ${tag} is installed. Start it from your application menu, or run: manang"
log "LaTeX needs a TeX installation (TeX Live); Typst and Markdown work out of the box."
