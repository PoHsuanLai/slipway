#!/bin/sh
# slipway-core stays pure: no network, D-Bus, async runtime or archive I/O.
set -eu
cd "$(dirname "$0")/.."
bad=$(cargo tree -p slipway-core -e normal --prefix none 2>/dev/null | grep -E '^(ureq|zbus|tokio|tar|zstd|rustls) ' || true)
[ -z "$bad" ] || { echo "check-boundary: slipway-core reaches: $bad"; exit 1; }
echo "check-boundary: ok"
