#!/usr/bin/env bash
# Install Manang on Linux from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/Fundaments-Work/Manang/main/scripts/install-linux.sh | bash
#
# Picks .deb on Debian-like systems, .rpm on Fedora/openSUSE-like systems and the
# tarball everywhere else (or with --user, which needs no root). The download
# is checked against the release's SHA256SUMS.txt before anything is installed.
#
# The tarball is used rather than an AppImage on purpose: Tauri's AppImage
# bundles a private copy of WebKitGTK, which made it 120 MB against a 46 MB
# .deb and left a browser engine frozen at build time. The tarball needs the
# system's WebKitGTK 4.1, exactly like the packages do.
#
# Manang is in beta, so the default is the newest release, pre-releases
# included. --stable refuses pre-releases, --beta asks for the newest one.
#
# Needs: curl, sha256sum. No jq, no bsdtar.
set -Eeuo pipefail

GH_REPO="Fundaments-Work/Manang"
API_BASE="${MANANG_API_BASE:-https://api.github.com/repos/${GH_REPO}}"
APP_DIR="${HOME}/.local/lib/manang"
BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"

log() { printf '%s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

usage() {
    cat <<'EOF'
Usage: install-linux.sh [options]

  --version V       Install this release: 0.1.0-beta.1, a tag (v0.1.0-beta.1),
                    "beta" (the newest pre-release) or "latest" (any release)
  --channel C       Which release to install when no version is given:
                    latest (newest, betas included), beta (newest pre-release),
                    stable (newest release that is not a pre-release)
  --stable          Shorthand for --channel stable
  --beta            Shorthand for --channel beta
  --bundle TYPE     deb, rpm or tar (default: chosen from your distribution)
  --user            Install from the tarball into ~/.local, no root needed,
                    whatever the distribution
  --list            Print every published release, newest first, and exit
  --print-latest    Print the tag the default would install, and exit
  --dry-run         Show what would be downloaded and installed, change nothing
  -h, --help        This text

Environment: GITHUB_TOKEN (optional) raises GitHub's rate limit;
MANANG_API_BASE (optional) points the script at another API for testing.

While Manang is in beta, "latest" is a pre-release: --channel stable installs
nothing yet and says so. A pre-release only reaches people who ask for it:
the script never installs one on its own if --stable was passed.

Remove it again with scripts/uninstall-linux.sh.
EOF
}

VERSION=""
CHANNEL="latest"
BUNDLE="auto"
USER_INSTALL=false
DRY_RUN=false
PRINT_LATEST=false
LIST=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || die "--version needs a value"; VERSION="${2#v}"; shift 2 ;;
        --channel) [[ $# -ge 2 ]] || die "--channel needs a value"; CHANNEL="$2"; shift 2 ;;
        --stable) CHANNEL="stable"; shift ;;
        --beta) CHANNEL="beta"; shift ;;
        --bundle|-b) [[ $# -ge 2 ]] || die "--bundle needs a value"; BUNDLE="$2"; shift 2 ;;
        --user) USER_INSTALL=true; shift ;;
        --list) LIST=true; shift ;;
        --print-latest) PRINT_LATEST=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown argument: $1" ;;
    esac
done

case "$CHANNEL" in
    latest|beta|stable) ;;
    *) die "unknown channel '$CHANNEL' (use latest, beta or stable)" ;;
esac

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required (coreutils)"
[[ "$(uname -s)" == "Linux" ]] || die "this script is for Linux; see docs/user/install.md for other systems"
case "$(uname -m)" in
    x86_64|amd64) ;;
    *) die "no Linux build for $(uname -m) yet (x86-64 only); see docs/user/install.md" ;;
esac

# ── Which release ──
# MANANG_API_BASE is for testing the script against a local copy of the API.
# GITHUB_TOKEN (a token with no scopes is enough) lifts the anonymous rate limit
# of 60 requests an hour, which matters on shared addresses and in CI.
#
# `api` fills API_BODY instead of printing, so a failure can carry its own
# message (API_ERR) and stop the script once, instead of once per subshell.
API_BODY=""
API_ERR=""
api() {
    local code
    local -a auth=()
    API_BODY=""
    API_ERR=""
    if [[ -n "${GITHUB_TOKEN:-}" ]]; then
        auth=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
    fi
    if API_BODY="$(curl -fsSL -H 'Accept: application/vnd.github+json' ${auth[@]+"${auth[@]}"} "${API_BASE}/$1" 2>/dev/null)"; then
        return 0
    fi
    code="$(curl -sS -o /dev/null -w '%{http_code}' ${auth[@]+"${auth[@]}"} "${API_BASE}/$1" 2>/dev/null || echo 000)"
    case "$code" in
        401) API_ERR="GitHub refused GITHUB_TOKEN. Unset it to install anonymously." ;;
        403 | 429)
            API_ERR="GitHub's rate limit for this address is used up. Wait an hour, or set GITHUB_TOKEN to a token with no scopes: GITHUB_TOKEN=... bash install-linux.sh" ;;
        000) API_ERR="could not reach GitHub. Check the network, then try again." ;;
        *) API_ERR="" ;;
    esac
    return 1
}

field() {
    printf '%s' "$1" | grep -o "\"$2\"[[:space:]]*:[[:space:]]*[^,}]*" | head -n1 \
        | sed -E "s/\"$2\"[[:space:]]*:[[:space:]]*//; s/^\"(.*)\"\$/\\1/"
}

# One line per release, newest first, from the list endpoint: the tag and
# whether GitHub marks it as a pre-release. GitHub orders the list by created
# date and writes "tag_name" before "prerelease", so scanning the document for
# those two keys in order finds each release exactly once, whatever the
# nesting and line breaks of the rest of the JSON.
scan_releases() {
    printf '%s' "$API_BODY" | awk -v kind="$1" '
        function flush(  pre) {
            if (tag == "") return
            pre = preflag ? "beta" : "stable"
            # Only releases that actually carry an installer. The same tag
            # list also holds the LaTeX engine releases (tagged
            # tectonic-<version>), which are downloads for the app and not
            # something anybody can install.
            if (have && (kind == "" || kind == "latest" || kind == pre)) {
                print tag "\t" pre
            }
        }
        /"tag_name"[[:space:]]*:/ {
            flush()
            tag = $0
            sub(/.*"tag_name"[[:space:]]*:[[:space:]]*"/, "", tag)
            sub(/".*/, "", tag)
            preflag = 0
            have = 0
            pending = 1
            # GitHub pretty-prints and puts "prerelease" on a later line; a
            # compact reply has both on this one.
            if ($0 ~ /"prerelease"[[:space:]]*:[[:space:]]*(true|false)/) read_pre()
            next
        }
        function read_pre() {
            preflag = ($0 ~ /"prerelease"[[:space:]]*:[[:space:]]*true/) ? 1 : 0
            pending = 0
        }
        pending && /"prerelease"[[:space:]]*:[[:space:]]*(true|false)/ { read_pre() }
        # Asset URLs only: the release notes live in one JSON string, where
        # every quote is escaped and cannot match this.
        /"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*\.(deb|rpm|tar\.xz|dmg|msi|exe)"/ { have = 1 }
        END { flush() }'
}

# Which tag does this channel mean? `latest` is the newest release whatever it
# is, `beta` the newest pre-release, `stable` the newest release that GitHub
# does not mark as a pre-release. PICKED is a global so that a failure stops
# the script once, not once per subshell.
PICKED=""
pick_tag() {
    api "releases?per_page=100" || return 1
    PICKED="$(scan_releases "$1" | head -n1 | cut -f1 | sed -E 's/^v//')"
}

if [[ "$LIST" == true ]]; then
    api "releases?per_page=100" || die "${API_ERR:-could not read the release list from GitHub}"
    scan_releases "" | while IFS=$'\t' read -r t kind; do
        printf '%-16s %s\n' "$t" "$kind"
    done
    exit 0
fi

case "$VERSION" in
    latest) VERSION="" ;;
    beta)
        pick_tag beta || die "${API_ERR:-could not read the release list from GitHub}"
        VERSION="$PICKED"
        [[ -n "$VERSION" ]] || die "no pre-release has been published yet"
        ;;
    *) ;;
esac
if [[ -z "$VERSION" ]]; then
    pick_tag "$CHANNEL" || die "${API_ERR:-could not read the release list from GitHub}"
    VERSION="$PICKED"
    [[ -n "$VERSION" ]] || die "no ${CHANNEL} release has been published yet; see --list"
fi

# Releases are tagged v<version>; accept a tag without the prefix too.
if ! api "releases/tags/v${VERSION}" && ! api "releases/tags/${VERSION}"; then
    die "${API_ERR:-no release v${VERSION} found (see --list for what is published)}"
fi
release_json="$API_BODY"
tag="$(field "$release_json" tag_name)"
[[ -n "$tag" ]] || die "no release found"
if [[ "$PRINT_LATEST" == true ]]; then
    printf '%s\n' "$tag"
    exit 0
fi
if [[ "$(field "$release_json" prerelease)" == "true" ]]; then
    note=" (pre-release)"
else
    note=""
fi

asset_url() {
    printf '%s' "$release_json" \
        | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | sed -E 's/.*"([a-z]+:[^"]*)"$/\1/' \
        | grep -E "$1" | head -n1 || true
}

# ── Which format ──
if [[ "$BUNDLE" == "auto" ]]; then
    if [[ "$USER_INSTALL" == true ]]; then BUNDLE="tar"
    elif command -v apt-get >/dev/null 2>&1 || command -v dpkg >/dev/null 2>&1; then BUNDLE="deb"
    elif command -v dnf >/dev/null 2>&1 || command -v zypper >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; then BUNDLE="rpm"
    else BUNDLE="tar"
    fi
fi
case "$BUNDLE" in
    deb) pattern='\.deb$' ;;
    rpm) pattern='\.rpm$' ;;
    tar) pattern='_amd64\.tar\.xz$' ;;
    *) die "unknown bundle '$BUNDLE' (use deb, rpm or tar)" ;;
esac

url="$(asset_url "$pattern")"
[[ -n "$url" ]] || die "release ${tag} has no ${BUNDLE} file; see https://github.com/${GH_REPO}/releases/tag/${tag}"
sums_url="$(asset_url 'SHA256SUMS\.txt$')"
file="${url##*/}"

log "Manang ${tag}${note}: ${file} (${BUNDLE})"
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

install_tar() {
    # The tarball carries usr/bin/manang, a .desktop file and the icons, and
    # needs WebKitGTK 4.1 from the system (Debian/Ubuntu: libwebkit2gtk-4.1-0,
    # Fedora: webkit2gtk4.1, Arch: webkit2gtk-4.1), the same as the packages.
    command -v tar >/dev/null 2>&1 || die "tar is required for the tarball; use --bundle deb or rpm instead"
    if ! tar -tJf "${work}/${file}" >/dev/null 2>&1; then
        die "tar cannot read ${file}; install xz-utils (Debian/Ubuntu) or use --bundle deb"
    fi
    mkdir -p "$APP_DIR" "$BIN_DIR" "$APPS_DIR"
    rm -rf "${work}/unpack"
    mkdir -p "${work}/unpack"
    tar -C "${work}/unpack" -xJf "${work}/${file}"
    [[ -x "${work}/unpack/usr/bin/manang" ]] || die "${file} does not contain usr/bin/manang"
    install -Dm755 "${work}/unpack/usr/bin/manang" "${BIN_DIR}/manang"
    # Every icon size goes into its own hicolor directory. Filing the 32x32
    # image under 128x128 (and dropping the 256 and 512 sizes) is why the
    # installed icon looked wrong or did not show in the window list. The
    # tarball ships 32x32, 128x128 and 256x256@2; fetch the rest from the tag.
    for entry in \
        "32x32:src-tauri/icons/32x32.png" \
        "128x128:src-tauri/icons/128x128.png" \
        "256x256:src-tauri/icons/128x128@2x.png" \
        "256x256@2:src-tauri/icons/128x128@2x.png" \
        "512x512:src-tauri/icons/icon.png"; do
        size="${entry%%:*}"
        source_path="${entry#*:}"
        target="${HOME}/.local/share/icons/hicolor/${size}/apps/manang.png"
        packaged="${work}/unpack/usr/share/icons/hicolor/${size}/apps/manang.png"
        if [[ -f "${packaged}" ]]; then
            install -Dm644 "${packaged}" "${target}"
        else
            mkdir -p "${target%/*}"
            if curl -fsSL -o "${target}.part" \
                "https://raw.githubusercontent.com/${GH_REPO}/${tag}/${source_path}"; then
                mv "${target}.part" "${target}"
            else
                rm -f "${target}.part"
                log "note: could not fetch the ${size} icon; launchers will scale another size"
            fi
        fi
    done
    cat > "${APPS_DIR}/manang.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Manang
Comment=Write papers, theses and slides in Typst, LaTeX or Markdown, and draw the figures too.
Exec=${BIN_DIR}/manang %F
Icon=manang
Terminal=false
Categories=Office;Education;Science;
StartupWMClass=manang
DESKTOP
    update-desktop-database "$APPS_DIR" 2>/dev/null || true
    gtk-update-icon-cache -q "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
    # A running GNOME Shell stats icon theme directories and reloads them when
    # one changes; a new file inside an existing size directory changes no
    # directory it stats, so touch the theme directory to pick up the icons
    # without logging out.
    touch "${HOME}/.local/share/icons/hicolor" "${HOME}/.local/share/icons" 2>/dev/null || true
    log "Installed ${BIN_DIR}/manang for ${USER}"
    case ":${PATH}:" in
        *":${BIN_DIR}:"*) ;;
        *) log "note: ${BIN_DIR} is not on your PATH; start Manang from the application menu or add it" ;;
    esac
    # grep without -q on purpose: -q exits at the first match, and ldconfig
    # then gets SIGPIPE while still writing its cache, which pipefail turns
    # into a false "WebKitGTK is missing" on machines that have it.
    if ! ldconfig -p 2>/dev/null | grep 'libwebkit2gtk-4\.1' >/dev/null; then
        log "note: Manang needs WebKitGTK 4.1 from your system (Debian/Ubuntu: libwebkit2gtk-4.1-0, Fedora: webkit2gtk4.1, Arch: webkit2gtk-4.1). Without it, run manang and read the error it prints."
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
    tar) install_tar ;;
esac

log ""
log "Manang ${tag} is installed. Start it from your application menu, or run: manang"
if [[ "$(field "$release_json" prerelease)" == "true" ]]; then
    log "This is a pre-release: bugs are expected. Install a marked stable release with --stable once one exists."
fi
log "Typst and Markdown work out of the box. LaTeX uses a TeX installation (TeX Live, MiKTeX or MacTeX) if you have one;"
log "otherwise open a .tex file and use Preview > Download the built-in engine (Linux only, about 48 MB, once)."
