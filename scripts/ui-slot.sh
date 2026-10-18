#!/usr/bin/env bash
# Run a command (normally the UI suite) in one of a few "slots", so that at most
# MANANG_UI_SLOTS (default 2) headless-Chromium suites run at once on this
# machine. A full suite takes about ten minutes; one global lock made every
# agent wait behind it. Section-only runs (MANANG_ONLY=...) are short and light
# and do not need a slot.
#
#   scripts/ui-slot.sh env MANANG_ORIGIN=http://127.0.0.1:1431 node scripts/verify-ui.mjs
set -u
slots="${MANANG_UI_SLOTS:-2}"
# The slot files live in the repository's shared git directory, so every
# worktree (the lead's checkout and each agent's) counts against the same slots.
here="$(cd "$(dirname "$0")/.." && pwd)"
common="$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
dir="${MANANG_UI_SLOT_DIR:-${common:-$here/.claude}/manang-ui-slots}"
mkdir -p "$dir"
while true; do
  for i in $(seq 1 "$slots"); do
    flock -n -E 200 "$dir/ui.slot.$i" "$@"
    rc=$?
    # 200 means this slot was busy; anything else is the command's own result.
    if [ "$rc" -ne 200 ]; then exit "$rc"; fi
  done
  sleep 3
done
