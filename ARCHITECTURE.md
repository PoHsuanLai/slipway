# Architecture

slipway keeps the quire desktop (the casement compositor, the sill shell and the apps) up to date
from prebuilt, signed release bundles that CI builds from the desktop repository's manifest and
publishes on the public releases repository `PoHsuanLai/desktop-releases`. It checks once a day,
tells the person when something newer exists, shows it on the Software Update page of Settings, and
installs only when the person clicks; then the interface asks them to log out. An install is atomic
and can be rolled back. The behaviour follows macOS Software Update.

## 1. Crates and allowed edges

| Crate | Purpose | I/O |
| --- | --- | --- |
| `slipway-core` | `Release` (parse and validate `release.toml`), `BundlePath`, version `standing`, signature check (`Authenticator`, `PublicKey`), digests, `Audit` (the bundle against the release), the install `Plan`, `plan_exports`, `next_check` (the schedule), `ProgressGate`, `decide_boot` (the health rule) | none |
| `slipway` | the privileged side (`apply`), downloading (`fetch`), the session service (`service`), the command line (`cli`), the daemon's files (`config`, `state`, `dirs`), and two binaries: `slipway` and `slipway-apply` | files, network, session bus, a child process |

`slipway-core` reaches no network, bus, runtime or archive crate (`scripts/check-boundary.sh`). It
reads no clock, environment or file: the caller passes bytes, times and what it saw on disk.

## 2. Layout

```
crates/slipway-core/src/{lib, version, digest, sign, path, audit, exports, health, schedule, progress}.rs
                        release/{mod, error, file, export}.rs      plan/{mod, prune}.rs
crates/slipway/src/{lib, main, config, dirs, state, layout, fsutil, keys, clock, elevate, problem, health, bootstrap}.rs
                   bin/slipway_apply.rs
                   apply/{mod, error, staged, unpack, step, install, exports, lock}.rs
                   fetch/{mod, feed, stage, ureq_fetch}.rs
                   service/{mod, context, engine, object, notify, run, status}.rs
                   cli/{mod, args, client, wait, confirm}.rs
dist/   slipway.service  org.quire.slipway.policy
scripts/install.sh  scripts/check-boundary.sh
```

## 3. The release format (frozen with the CI lane)

A release is the tag `v<semver>` on the releases repository with three assets, fetched from
`<index>/latest/download/<name>` (newest) or `<index>/download/v<version>/<name>` (one version):

- `release.toml` and `release.toml.minisig` (minisign signature of exactly those bytes);
- `bundle.tar.zst`: the tree to install, paths relative to the release root.

`release.toml` holds `format = 1`, `version`, `date` (RFC 3339), `notes`, `bundle = { name, sha256,
size }`, informational `[components]`, one `[[file]]` (`path`, `sha256`, `mode`) per regular file in
the bundle, and `[[export]]` (`link`, `target`) per link the system needs. `slipway-core::Release`
validates all of it: simple paths only (no absolute, `..`, empty or backslash parts, nothing under
`.slipway/`), unique files with none inside another, modes that are plain permission bits readable
by the owner and not writable by anyone else, no setuid; export links only under the seven allowed
directories, unique, naming something the bundle holds.

Trust anchor: a minisign public key, read from `/usr/share/slipway/release.pub` (the `pubkey_path`
setting names another) with the `SLIPWAY_RELEASE_PUBKEY` build-time text as the fallback when the
file does not exist. No key: nothing is installed and the person is told so.

## 4. Install layout and the switch

```
<root>/releases/<version>/            complete releases; .slipway/release.toml(+.minisig) is the verified copy
<root>/releases/<version>.partial/    an unpack in progress
<root>/current  -> releases/<version>
<root>/previous -> releases/<version>
<root>/lock
```

The root defaults to `/opt/quire`. `slipway-core::plan_install` turns (current, previous, releases,
target) into ordered steps: `Unpack`, `Publish` (rename `.partial` to the version), `SetPrevious`,
`SwitchCurrent`, `SyncExports`, `Prune`. Every prefix of the list leaves a working system: the new
tree is complete and flushed before the single `rename(2)` that changes what runs (`current.new` is
made, then renamed over `current`). A target not newer than `current` is refused; a rollback is the
same switch back with `current` and `previous` exchanged, so it can be undone. At most current,
previous and one unswitched newer release stay on disk.

Exports: after the switch each `[[export]]` link is made to point at `<root>/current/<target>`. A
link is created when missing, replaced only when it already points into the root, left alone and
reported when something else is there (a regular file, a link elsewhere), and removed when the
release being left listed it, the new one does not, and it still points into the root.
`slipway-core::plan_exports` decides from what the caller observed; `apply/exports.rs` observes and
acts. The export base (`/` on a real system) is a field of `Layout`, so tests use a temporary
directory.

## 5. Who runs as whom

```
person's session                              root (pkexec, polkit action org.quire.slipway.apply)
slipway run  --download, verify--> cache  --> slipway-apply <staged dir>   (re-verifies everything)
                                              slipway-apply --rollback
```

The daemon never writes under the root. It downloads into `$XDG_CACHE_HOME/slipway/<version>/`,
checks the signature of `release.toml` and then the bundle's size and digest, and only then asks
`slipway-apply` to install the directory. The helper trusts nothing but the path: it reads the files
once into memory, checks the signature with the key on this computer, the bundle digest, and the
digest of every file as it is written (so changing the staged bundle between the checks gains
nothing). It takes `<root>/lock`, unpacks into `<version>.partial` (regular files and plain
directories only, each created fresh, nothing through a link), flushes, then runs the plan. Its root
and key come from `/etc/slipway/config.toml`, never from the caller. Exit codes: 0 done, 1 failed,
2 usage, 3 refused (not newer / nothing to go back to), 4 did not verify, 5 busy, 6 no key.

## 6. The session service

`slipway run` owns `org.quire.Updates1` at `/org/quire/Updates1` on the session bus.

Properties (all emit `PropertiesChanged`): `State` (idle, checking, available, downloading, ready,
installing, installed, failed), `Current`, `Available` ("" when none), `Notes`, `Size` (bytes),
`Progress` (permille), `LastChecked` (unix seconds, 0 never), `Problem` (plain words).
Methods: `Check()`, `Install()`, `Rollback()`. Each returns at once, having already moved `State`
(so a caller that polls `State` never sees the old value after the call returns), and refuses with
a plain sentence when work is in progress; the work runs on a spawned task and the blocking parts
(ureq, the helper) on blocking threads.

`service/engine.rs` is the flow: check (latest `release.toml` verified, compared with the installed
version through `standing`, only `Newer` is offered), notify once per version, download with
`ProgressGate`-throttled progress, install through the `Elevate` seam, remember the install as
pending first login. The schedule (`next_check`) runs from `service/run.rs`: at start if overdue,
24 h plus up to an hour of per-machine jitter after a good check, an hour after a failed one.

State file `$XDG_STATE_HOME/slipway/state.toml` (last check and outcome, notified version, confirmed
version, pending marker) is written beside and renamed over. The configuration
`$XDG_CONFIG_HOME/slipway/config.toml` is only read.

## 7. Health and automatic rollback

An install leaves the marker `pending { version, stage = awaiting_first_login }`. The session runs
`slipway confirm` once the shell is up, which records the current version as confirmed. At every
start `health::on_start` applies `slipway-core::decide_boot`:

| marker | confirmed | action |
| --- | --- | --- |
| none | | nothing |
| for another version, or confirmed | | clear it |
| awaiting first login | no | move to first login seen |
| first login seen | no, previous exists | roll back (`slipway-apply --rollback`), clear, notify "The update could not start, so the previous version was restored." |
| first login seen | no, nothing to restore | clear it |

## 8. The seams

| Trait | Real | Test |
| --- | --- | --- |
| `fetch::Fetch` | `UreqFetch` (HTTPS only) | in-memory map (`tests/it/feed_stage.rs`) |
| `elevate::Elevate` | `Pkexec` | recording stand-in (`tests/it/health_start.rs`) |
| `slipway_core::Authenticator` | `PublicKey` (minisign) | accept-all and reject-all (`tests/it/support.rs`) |
| `Layout { root, export_base }` | `/opt/quire` and `/` | a temporary directory for both |

The D-Bus object and the notification code are not covered by the unit tests (no bus in a test);
they are checked by the orchestrator's session scenarios.
