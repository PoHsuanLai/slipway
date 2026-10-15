#!/bin/sh
# Install slipway's system pieces: the privileged helper, its polkit action, the user unit and,
# when given, the release key.
#
#   scripts/install.sh [--prefix DIR] [--destdir DIR] [--bin-dir DIR] [--no-build]
#                      [--root DIR] [--pubkey FILE]
#
# --prefix    where the software goes (default /usr; the environment variable PREFIX also sets it)
# --destdir   a staging root put in front of every path, for packagers and for trying the install
#             (default none; the environment variable DESTDIR also sets it)
# --bin-dir   install the already built slipway-apply found in DIR instead of building it
# --no-build  install from target/release (or $CARGO_TARGET_DIR/release) without building first
# --root      the install root the desktop lives under (default /opt/quire); the unit starts
#             ROOT/current/bin/slipway
# --pubkey    the minisign public key releases are signed with, installed as the trust anchor
#
# What goes where (DESTDIR, when given, is in front of all of it):
#   PREFIX/libexec/slipway-apply                       the privileged helper
#   PREFIX/share/polkit-1/actions/org.quire.slipway.policy   lets the person at the keyboard run it
#   PREFIX/lib/systemd/user/slipway.service            the daily check, part of the graphical session
#   PREFIX/share/slipway/release.pub                   the trust anchor (only with --pubkey)
#
# polkit reads actions from /usr/share/polkit-1/actions and the daemon looks for the helper at
# /usr/libexec/slipway-apply and the key at /usr/share/slipway/release.pub, so PREFIX should be /usr.
# A different prefix is for trying the install under a DESTDIR; the paths inside the policy are
# rewritten to match.
#
# The script is standalone and idempotent: run again, it changes nothing that is already as it
# should be. It writes nothing under a home directory, and only writes under /usr when run, without
# a destdir, by someone who may. The unit is not enabled (that writes under the home directory).
set -eu

prefix=${PREFIX:-/usr}
destdir=${DESTDIR:-}
bin_dir=
build=yes
root=/opt/quire
pubkey=

usage() {
    sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case $1 in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --destdir) destdir=${2:?--destdir needs a directory}; shift 2 ;;
        --destdir=*) destdir=${1#--destdir=}; shift ;;
        --bin-dir) bin_dir=${2:?--bin-dir needs a directory}; shift 2 ;;
        --bin-dir=*) bin_dir=${1#--bin-dir=}; shift ;;
        --no-build) build=no; shift ;;
        --root) root=${2:?--root needs a directory}; shift 2 ;;
        --root=*) root=${1#--root=}; shift ;;
        --pubkey) pubkey=${2:?--pubkey needs a file}; shift 2 ;;
        --pubkey=*) pubkey=${1#--pubkey=}; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "install.sh: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
done

case $prefix in
    /*) ;;
    *) echo "install.sh: the prefix must be an absolute path, not '$prefix'" >&2; exit 2 ;;
esac
case $root in
    /*) ;;
    *) echo "install.sh: the root must be an absolute path, not '$root'" >&2; exit 2 ;;
esac
if [ -n "$pubkey" ] && [ ! -f "$pubkey" ]; then
    echo "install.sh: the key file '$pubkey' is not there" >&2
    exit 1
fi
prefix=${prefix%/}
root=${root%/}
destdir=${destdir%/}

here=$(cd "$(dirname "$0")/.." && pwd)
dist=$here/dist
changed=0

mode_of() {
    stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"
}

# put MODE SOURCE TARGET [FROM TO]...: write SOURCE to DESTDIR/TARGET with mode MODE, replacing the
# text FROM by TO on the way for each FROM TO pair given. An existing target with the same content
# and mode is left alone.
put() {
    mode=$1
    source=$2
    target=$destdir$3
    shift 3
    mkdir -p "$(dirname "$target")"
    staged=$target.new.$$
    cp "$source" "$staged"
    while [ $# -ge 2 ]; do
        FROM=$1 TO=$2 awk '
            BEGIN { from = ENVIRON["FROM"]; to = ENVIRON["TO"]; n = length(from) }
            {
                out = ""; line = $0
                while ((i = index(line, from)) > 0) {
                    out = out substr(line, 1, i - 1) to
                    line = substr(line, i + n)
                }
                print out line
            }' "$staged" >"$staged.sub"
        mv -f "$staged.sub" "$staged"
        shift 2
    done
    chmod "$mode" "$staged"
    if [ -f "$target" ] && cmp -s "$staged" "$target" && [ "$(mode_of "$target")" = "$(mode_of "$staged")" ]; then
        rm -f "$staged"
    else
        mv -f "$staged" "$target"
        changed=$((changed + 1))
        echo "installed ${target#"$destdir"}"
    fi
}

if [ -n "$bin_dir" ]; then
    built=$bin_dir
else
    if [ "$build" = yes ]; then
        (cd "$here" && cargo build --release --locked -p slipway --bin slipway-apply)
    fi
    built=${CARGO_TARGET_DIR:-$here/target}/release
fi
if [ ! -x "$built/slipway-apply" ]; then
    echo "install.sh: $built/slipway-apply is not there; build first or point --bin-dir at it" >&2
    exit 1
fi

# The files in dist/ name /usr/libexec/slipway-apply and /opt/quire; different choices change those paths.
libexec=$prefix/libexec
put 755 "$built/slipway-apply" "$libexec/slipway-apply"
put 644 "$dist/org.quire.slipway.policy" "$prefix/share/polkit-1/actions/org.quire.slipway.policy" \
    /usr/libexec/slipway-apply "$libexec/slipway-apply"
put 644 "$dist/slipway.service" "$prefix/lib/systemd/user/slipway.service" /opt/quire "$root"
if [ -n "$pubkey" ]; then
    put 644 "$pubkey" "$prefix/share/slipway/release.pub"
fi

if [ "$changed" -eq 0 ]; then
    echo "nothing to change"
fi
