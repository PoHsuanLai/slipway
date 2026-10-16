#!/bin/sh
# The two edges of the dependency graph:
#   slipway-core stays pure: no network, D-Bus, async runtime or archive I/O.
#   slipway-root is what runs as root (slipway-apply, slipway-rollback, slipway-health): it may read
#   archives, but must never reach the network client, TLS, the bus or the async runtime.
set -eu
cd "$(dirname "$0")/.."

# reaches CRATE PATTERN: the dependencies of CRATE (normal ones, build scripts excluded) whose name
# matches the extended regular expression PATTERN. A failing `cargo tree` stops the script instead of
# reading as "nothing found".
reaches() {
    tree=$(cargo tree -p "$1" -e normal --prefix none)
    echo "$tree" | grep -E "^($2) " || true
}

bad=$(reaches slipway-core 'ureq|zbus|tokio|tar|zstd|rustls')
[ -z "$bad" ] || { echo "check-boundary: slipway-core reaches: $bad"; exit 1; }

bad=$(reaches slipway-root 'ureq|rustls|zbus|tokio')
[ -z "$bad" ] || { echo "check-boundary: slipway-root reaches: $bad"; exit 1; }

echo "check-boundary: ok"
