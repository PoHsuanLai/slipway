#!/bin/sh
# Install slipway's system pieces: the privileged helpers, their polkit actions, the user unit, the bus
# activation file, the system settings file and, when given, the release key.
#
#   scripts/install.sh [--prefix DIR] [--destdir DIR] [--sysconfdir DIR] [--bin-dir DIR] [--no-build]
#                      [--root DIR] [--pubkey FILE]
#
# --prefix    where the software goes (default /usr; the environment variable PREFIX also sets it)
# --destdir   a staging root put in front of every path, for packagers and for trying the install
#             (default none; the environment variable DESTDIR also sets it)
# --sysconfdir where the system settings go (default /etc when the prefix is /usr, else PREFIX/etc;
#             the environment variable SYSCONFDIR also sets it)
# --bin-dir   install the already built slipway-apply, slipway-rollback and slipway-health found in DIR
#             instead of building them
# --no-build  install from target/release (or $CARGO_TARGET_DIR/release) without building first
#
# Building and installing are two steps with two owners. The build runs as the person (it compiles
# every dependency, and none of that may run as root); the install is the step that needs root, and it
# only places finished files: with --bin-dir or --no-build it never runs cargo. Asked to build while
# running as root the script refuses and says what to run instead:
#
#   cargo build --release --locked -p slipway-root       (as yourself, in this folder)
#   sudo sh -c 'umask 022 && scripts/install.sh --bin-dir target/release --pubkey FILE'
#
# --root      the install root the desktop lives under (default /opt/quire); the unit starts
#             ROOT/current/bin/slipway and the system settings name it
# --pubkey    the minisign public key releases are signed with, installed as the trust anchor
#
# What goes where (DESTDIR, when given, is in front of all of it):
#   PREFIX/libexec/slipway-apply                       the privileged helper that installs a release
#   PREFIX/libexec/slipway-rollback                    the privileged helper that goes back (asks for a password)
#   PREFIX/libexec/slipway-health                      the privileged half of the health check
#   PREFIX/share/polkit-1/actions/org.quire.slipway.policy   lets the person at the keyboard run them
#   PREFIX/lib/systemd/user/slipway.service            the daily check, part of the graphical session
#   PREFIX/share/dbus-1/services/org.quire.Updates1.service
#                                                      starts that unit when the update service is called
#   SYSCONFDIR/slipway/config.toml                     the system settings: root and key path, read by
#                                                      the helper and by the daemon (never overwritten)
#   PREFIX/share/slipway/release.pub                   the trust anchor (only with --pubkey)
#
# The system settings file is the one place the install root and the key path are written down: the
# privileged helper reads only it and the daemon reads it first, so they cannot disagree. A file that
# is already there is kept as it is (the person or an earlier install chose it); if it names another
# root than --root, the script says so. Both programs look for it at /etc/slipway/config.toml, so
# SYSCONFDIR should be /etc outside a DESTDIR trial.
#
# polkit reads actions from /usr/share/polkit-1/actions and the daemon looks for the helper at
# /usr/libexec/slipway-apply and the key at /usr/share/slipway/release.pub, so PREFIX should be /usr.
# A different prefix is for trying the install under a DESTDIR; the paths inside the policy are
# rewritten to match.
#
# On a computer that uses SELinux (Fedora and its relatives), and not under a DESTDIR, the script also tells
# the policy how to label the install root, so system services may run programs from it:
# `semanage fcontext` gives ROOT(/.*)? the type usr_t and ROOT(/.*)?/bin(/.*)? bin_t, then `restorecon`
# applies it. Both programs must be installed and the script must run as root; otherwise it prints
# what to run. Nothing is done where SELinux is off or the tools are missing. slipway-apply also runs
# `restorecon` on every release it places, so later releases need nothing from this script.
#
# Every folder is made with `install -d -m755` and every file placed with `install -m MODE`, so a
# restrictive umask (root's is often 077) changes no mode and there is no moment when a file has the
# wrong one; the script also sets umask 022 for anything else it creates.
#
# The script is standalone and idempotent: run again, it changes nothing that is already as it
# should be. It writes nothing under a home directory, and only writes under /usr when run, without
# a destdir, by someone who may. The unit is not enabled (that writes under the home directory).
set -eu
umask 022

prefix=${PREFIX:-/usr}
destdir=${DESTDIR:-}
bin_dir=
build=yes
root=/opt/quire
pubkey=
sysconfdir=${SYSCONFDIR:-}

usage() {
    sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case $1 in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --destdir) destdir=${2:?--destdir needs a directory}; shift 2 ;;
        --destdir=*) destdir=${1#--destdir=}; shift ;;
        --sysconfdir) sysconfdir=${2:?--sysconfdir needs a directory}; shift 2 ;;
        --sysconfdir=*) sysconfdir=${1#--sysconfdir=}; shift ;;
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
case $sysconfdir in
    '' | /*) ;;
    *) echo "install.sh: the sysconfdir must be an absolute path, not '$sysconfdir'" >&2; exit 2 ;;
esac
# The settings file is TOML written by this script, so the two paths in it must not need escaping.
for path in "$prefix" "$root"; do
    case $path in
        *\"* | *\\* | *'
'*) echo "install.sh: '$path' holds a character the settings file cannot hold" >&2; exit 2 ;;
    esac
done
if [ -n "$pubkey" ] && [ ! -f "$pubkey" ]; then
    echo "install.sh: the key file '$pubkey' is not there" >&2
    exit 1
fi
prefix=${prefix%/}
root=${root%/}
destdir=${destdir%/}
if [ -z "$sysconfdir" ]; then
    if [ "$prefix" = /usr ]; then sysconfdir=/etc; else sysconfdir=$prefix/etc; fi
fi
sysconfdir=${sysconfdir%/}

here=$(cd "$(dirname "$0")/.." && pwd)
dist=$here/dist
changed=0

mode_of() {
    stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"
}

# A private folder for the text this script composes before it is placed; removed on the way out.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

# make_dir DIR: DIR and any missing parents, each made with `install -d -m755`. A folder that is already
# there is left as it is (this never changes the mode of /usr/lib or the like).
make_dir() {
    [ -d "$1" ] && return 0
    make_dir "$(dirname "$1")"
    install -d -m755 "$1"
}

# substitute FILE FROM TO: replaces every FROM in FILE by TO, in place.
substitute() {
    FROM=$2 TO=$3 awk '
        BEGIN { from = ENVIRON["FROM"]; to = ENVIRON["TO"]; n = length(from) }
        {
            out = ""; line = $0
            while ((i = index(line, from)) > 0) {
                out = out substr(line, 1, i - 1) to
                line = substr(line, i + n)
            }
            print out line
        }' "$1" >"$1.sub"
    mv -f "$1.sub" "$1"
}

# place MODE SOURCE TARGET: SOURCE becomes TARGET with mode MODE, written beside it with `install -m` and
# renamed over it. An existing target with the same content and mode is left alone.
place() {
    staged=$3.new.$$
    install -m "$1" "$2" "$staged"
    if [ -f "$3" ] && cmp -s "$staged" "$3" && [ "$(mode_of "$3")" = "$(mode_of "$staged")" ]; then
        rm -f "$staged"
        return 1
    fi
    mv -f "$staged" "$3"
}

# put MODE SOURCE TARGET [FROM TO]...: write SOURCE to DESTDIR/TARGET with mode MODE, replacing the
# text FROM by TO on the way for each FROM TO pair given.
put() {
    mode=$1
    source=$2
    target=$destdir$3
    shift 3
    make_dir "$(dirname "$target")"
    if [ $# -ge 2 ]; then
        cp "$source" "$work/text"
        while [ $# -ge 2 ]; do
            substitute "$work/text" "$1" "$2"
            shift 2
        done
        source=$work/text
    fi
    if place "$mode" "$source" "$target"; then
        changed=$((changed + 1))
        echo "installed ${target#"$destdir"}"
    fi
}

if [ -n "$bin_dir" ]; then
    built=$bin_dir
else
    if [ "$build" = yes ]; then
        if [ "$(id -u)" = 0 ]; then
            echo "install.sh: not building as root: compiling the dependencies must not run with administrator rights." >&2
            echo "install.sh: build as yourself:  cargo build --release --locked -p slipway-root" >&2
            echo "install.sh: then install:       sudo sh -c 'umask 022 && $0 --bin-dir target/release${pubkey:+ --pubkey $pubkey}'" >&2
            exit 1
        fi
        (cd "$here" && cargo build --release --locked -p slipway-root)
    fi
    built=${CARGO_TARGET_DIR:-$here/target}/release
fi
for helper in slipway-apply slipway-rollback slipway-health; do
    if [ ! -x "$built/$helper" ]; then
        echo "install.sh: $built/$helper is not there; build first or point --bin-dir at it" >&2
        exit 1
    fi
done

# The files in dist/ name /usr/libexec/slipway-apply and /opt/quire; different choices change those paths.
libexec=$prefix/libexec
put 755 "$built/slipway-apply" "$libexec/slipway-apply"
put 755 "$built/slipway-rollback" "$libexec/slipway-rollback"
put 755 "$built/slipway-health" "$libexec/slipway-health"
put 644 "$dist/org.quire.slipway.policy" "$prefix/share/polkit-1/actions/org.quire.slipway.policy" \
    /usr/libexec/slipway-apply "$libexec/slipway-apply" \
    /usr/libexec/slipway-rollback "$libexec/slipway-rollback" \
    /usr/libexec/slipway-health "$libexec/slipway-health"
put 644 "$dist/slipway.service" "$prefix/lib/systemd/user/slipway.service" /opt/quire "$root"
put 644 "$dist/org.quire.Updates1.service" "$prefix/share/dbus-1/services/org.quire.Updates1.service" \
    /opt/quire "$root"
if [ -n "$pubkey" ]; then
    put 644 "$pubkey" "$prefix/share/slipway/release.pub"
fi

# The system settings, written once. An existing file is never touched.
settings=$destdir$sysconfdir/slipway/config.toml
if [ -e "$settings" ]; then
    kept=$(sed -n 's/^root[[:space:]]*=[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$settings" | tail -n 1)
    echo "kept ${settings#"$destdir"}"
    if [ -n "$kept" ] && [ "$kept" != "$root" ]; then
        echo "install.sh: note: it names the root $kept, not $root; edit it if $root is meant" >&2
    fi
else
    make_dir "$(dirname "$settings")"
    {
        echo "# slipway system settings: read by slipway-apply (as root) and by the update service."
        echo "# The person's own file may only change settings_command and name."
        echo "root = \"$root\""
        echo "pubkey_path = \"$prefix/share/slipway/release.pub\""
    } >"$work/config.toml"
    place 644 "$work/config.toml" "$settings" || true
    changed=$((changed + 1))
    echo "installed ${settings#"$destdir"}"
fi

# SELinux labels for the install root (see the header). Quiet and harmless where SELinux is not in use.
selinux_label_root() {
    [ -z "$destdir" ] || return 0
    [ -e /sys/fs/selinux/enforce ] || return 0
    command -v semanage >/dev/null 2>&1 || {
        echo "SELinux is on but semanage is not installed: not labelling $root (install policycoreutils-python-utils)"
        return 0
    }
    if [ "$(id -u)" != 0 ]; then
        echo "SELinux is on: as root, run: semanage fcontext -a -t bin_t '$root(/.*)?/bin(/.*)?' and semanage fcontext -a -t usr_t '$root(/.*)?', then restorecon -RF $root"
        return 0
    fi
    # -a adds a rule; if one exists already, -m modifies it, so running again changes nothing.
    for rule in "usr_t:$root(/.*)?" "bin_t:$root(/.*)?/bin(/.*)?"; do
        type=${rule%%:*}
        pattern=${rule#*:}
        semanage fcontext -a -t "$type" "$pattern" 2>/dev/null || semanage fcontext -m -t "$type" "$pattern"
        echo "SELinux: $pattern is labelled $type"
    done
    if command -v restorecon >/dev/null 2>&1 && [ -d "$root" ]; then
        restorecon -RF "$root"
        echo "SELinux: relabelled $root"
    fi
}
selinux_label_root

if [ "$changed" -eq 0 ]; then
    echo "nothing to change"
fi
