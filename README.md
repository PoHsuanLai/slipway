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
  user side hands it except a path.

Install the system pieces (helper, polkit action, user unit, bus activation file and the system
settings `/etc/slipway/config.toml` holding the root and the key path) with
`scripts/install.sh --root /opt/quire --pubkey release.pub`; `--help` lists the options, and an
existing settings file is never overwritten. Your own `~/.config/slipway/config.toml` may only set
`settings_command` and `name`.

See ARCHITECTURE.md.
