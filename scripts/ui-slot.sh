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
dir="$(cd "$(dirname "$0")/.." && pwd)/.claude"
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
