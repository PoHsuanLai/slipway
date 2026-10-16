# slipway

Keeps a desktop installed from signed, prebuilt release bundles up to date.

- Checks a release index once a day. When a newer release is there it says so (a notification and a
  Software Update page) and installs only when the person asks.
- Every bundle is signed (minisign format) and every file in it carries a SHA-256 digest; nothing
  unverified is ever unpacked into place.
- Releases install side by side under one root (`<root>/releases/<version>/`); `<root>/current` is a
  symlink switched with one atomic rename. The previous release stays, so a rollback is the same switch
  back. A release that does not reach a healthy session is rolled back on the next login, and is not
  offered again until a newer one is published.
- `slipway` (user): the daily check, download and verify, the session service and the command line.
  `slipway-apply` (privileged, started through polkit): re-verifies and switches; it trusts nothing the
  user side hands it except a path. It, `slipway-rollback` and `slipway-health` are built from the small
  crate `slipway-root`, which does not link the network client, the bus or the async runtime.

Install the system pieces (helper, polkit action, user unit, bus activation file and the system
settings `/etc/slipway/config.toml` holding the root and the key path) in two steps: build the three
privileged programs as yourself with `scripts/install.sh --build-only`, then install them as
root with
`sudo sh -c 'umask 022 && scripts/install.sh --bin-dir target/release --root /opt/quire --pubkey release.pub'`.
The script never builds as root, and builds with the computer's own linker (no clang or mold needed). `--help` lists the options, and an existing settings file is never
overwritten. Your own `~/.config/slipway/config.toml` may only set
`settings_command` and `name`.

The helper is installed by hand and only it decides which system files a release may place, so re-run
the two steps above whenever a release is said to add system files (the input service's are the latest);
an older helper refuses such a release whole. The update service answers `Install` and `Rollback` only for
the desktop's own programs, which it knows from the root-owned table `/etc/quire/desktop-callers.toml` that
the desktop's installer places; without that file it refuses both and says so in its log.

See ARCHITECTURE.md.
