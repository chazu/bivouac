#!/bin/sh
# Keeps target/ bounded. Run automatically by every Makefile target.
#  1. Deletes incremental-compilation caches untouched for TARGET_STALE_DAYS
#     (old ones pile up whenever a crate's hash changes).
#  2. If target/ is still over TARGET_CAP_MB, wipes it entirely.
set -eu

cd "$(dirname "$0")/.."
CAP_MB="${TARGET_CAP_MB:-3000}"
STALE_DAYS="${TARGET_STALE_DAYS:-3}"

[ -d target ] || exit 0

find target -mindepth 3 -maxdepth 3 -type d -path '*/incremental/*' \
    -mtime +"$STALE_DAYS" -exec rm -rf {} + 2>/dev/null || true

size_mb=$(du -sm target | cut -f1)
if [ "$size_mb" -gt "$CAP_MB" ]; then
    echo "trim-target: target/ is ${size_mb} MB (cap ${CAP_MB} MB), cleaning"
    cargo clean
fi
