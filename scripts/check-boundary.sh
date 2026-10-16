#!/bin/sh
# The edges of the dependency graph:
#   slipway-core stays pure: no network, D-Bus, async runtime or archive I/O.
#   slipway-root is what runs as root (slipway-apply, slipway-rollback, slipway-health). It may read
#   archives, but its whole tree (normal and build dependencies) holds no network client, TLS, bus,
#   async runtime, logging framework or user-level code, and its direct dependencies are exactly the
#   short list below. A new direct dependency of the root programs is a decision, so it is made here.
set -eu
cd "$(dirname "$0")/.."

# reaches CRATE KINDS PATTERN: the dependencies of CRATE (of the dependency KINDS cargo tree names;
# dev dependencies never ship) whose name matches the extended regular expression PATTERN. A failing
# `cargo tree` stops the script instead of reading as "nothing found".
reaches() {
    tree=$(cargo tree -p "$1" -e "$2" --prefix none)
    echo "$tree" | grep -E "^($3) " || true
}

bad=$(reaches slipway-core normal 'ureq|zbus|tokio|tar|zstd|rustls')
[ -z "$bad" ] || { echo "check-boundary: slipway-core reaches: $bad"; exit 1; }

# Excluded from the privileged programs: the user-level program, the network client and TLS, the bus,
# the async runtime and its building blocks, logging, and the caller-identity library.
excluded='slipway|peephole[a-z0-9_-]*|ureq[a-z0-9_-]*|rustls[a-z0-9_-]*|webpki[a-z0-9_-]*|native-tls|openssl[a-z0-9_-]*|ring'
excluded="$excluded|hyper[a-z0-9_-]*|h2|http|reqwest|zbus[a-z0-9_-]*|zvariant[a-z0-9_-]*|dbus[a-z0-9_-]*"
excluded="$excluded|tokio[a-z0-9_-]*|mio|async-[a-z0-9_-]*|futures[a-z0-9_-]*|smol|tracing[a-z0-9_-]*"
bad=$(reaches slipway-root normal,build "$excluded")
[ -z "$bad" ] || { echo "check-boundary: slipway-root reaches: $bad"; exit 1; }

# The direct dependencies of the privileged programs (the first line of the tree is the crate itself).
allowed='slipway-core|thiserror|semver|toml|rustix|tar|zstd'
direct=$(cargo tree -p slipway-root -e normal,build --depth 1 --prefix none | sed 1d | cut -d' ' -f1 | sort -u)
bad=$(echo "$direct" | grep -Ev "^($allowed)\$" || true)
[ -z "$bad" ] || { echo "check-boundary: slipway-root depends directly on: $bad"; exit 1; }

echo "check-boundary: ok"
