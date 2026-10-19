#!/usr/bin/env bash
# Remove Manang that scripts/install-linux.sh (or the .deb/.rpm) installed.
#
#   curl -fsSL https://raw.githubusercontent.com/Fundaments-Work/Manang/main/scripts/uninstall-linux.sh | bash
#
# Your projects are ordinary folders and are never touched. Settings, plugins and
# caches are kept unless you pass --purge.
set -Eeuo pipefail

log() { printf '%s\n' "$*" >&2; }

PURGE=false
for arg in "$@"; do
    case "$arg" in
        --purge) PURGE=true ;;
        -h|--help)
            printf 'Usage: uninstall-linux.sh [--purge]\n  --purge  also delete Manang settings, plugins, keys and caches\n'
            exit 0 ;;
        *) log "unknown argument: $arg"; exit 1 ;;
    esac
done

as_root() {
    if [[ "${EUID}" -eq 0 ]]; then "$@"
    elif command -v sudo >/dev/null 2>&1; then sudo "$@"
    else log "root is needed to remove the system package"; return 1
    fi
}

removed=false

# System packages.
if command -v dpkg >/dev/null 2>&1 && dpkg -s manang >/dev/null 2>&1; then
    if command -v apt-get >/dev/null 2>&1; then as_root apt-get remove -y manang; else as_root dpkg -r manang; fi
    removed=true
elif command -v rpm >/dev/null 2>&1 && rpm -q manang >/dev/null 2>&1; then
    if command -v dnf >/dev/null 2>&1; then as_root dnf remove -y manang
    elif command -v zypper >/dev/null 2>&1; then as_root zypper --non-interactive remove manang
    else as_root rpm -e manang; fi
    removed=true
fi

# The per-user AppImage install.
for path in \
    "${HOME}/.local/lib/manang" \
    "${HOME}/.local/bin/manang" \
    "${HOME}/.local/share/applications/manang.desktop" \
    "${HOME}/.local/share/icons/hicolor/128x128/apps/manang.png"; do
    if [[ -e "$path" || -L "$path" ]]; then
        rm -rf -- "$path"
        log "removed $path"
        removed=true
    fi
done
update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true

if [[ "$PURGE" == true ]]; then
    for dir in \
        "${XDG_CONFIG_HOME:-$HOME/.config}/work.fundaments.manang" \
        "${XDG_DATA_HOME:-$HOME/.local/share}/work.fundaments.manang" \
        "${XDG_CACHE_HOME:-$HOME/.cache}/work.fundaments.manang"; do
        if [[ -d "$dir" ]]; then rm -rf -- "$dir"; log "removed $dir"; fi
    done
fi

if [[ "$removed" == true ]]; then
    log "Manang is removed. Your projects were not touched."
else
    log "Nothing to remove: no Manang install was found."
fi
