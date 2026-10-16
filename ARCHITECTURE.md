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
| `slipway-root` | everything that runs as root: the verified unpack and atomic switch (`apply`), the three programs' entry points (`helper`) and binaries `slipway-apply`, `slipway-rollback`, `slipway-health`, the install root's layout (`layout`), small file operations (`fsutil`), the trust anchor (`keys`), the report the helper prints (`report`) and the two system settings it reads (`settings`) | files, a child process (`restorecon`) |
| `slipway` | downloading (`fetch`), the session service (`service`), the command line (`cli`), the daemon's files (`config`, `state`, `dirs`), the `pkexec` seam (`elevate`), and one binary: `slipway` | files, network, session bus, a child process |

`slipway-core` reaches no network, bus, runtime or archive crate. `slipway-root` may read archives (`tar`,
`zstd`) but reaches no network client, TLS, bus or async runtime (`ureq`, `rustls`, `zbus`, `tokio`):
the programs that run with administrator rights link only `slipway-core`, `thiserror`, `semver`, `toml`,
`rustix`, `tar` and `zstd` (and what those bring). `scripts/check-boundary.sh` checks both edges. `slipway`
depends on `slipway-root` (for `Layout`, the file operations, the key loader, the report and the helper's
exit codes), never the other way. `slipway-core` reads no clock, environment or file: the caller passes
bytes, times and what it saw on disk.

## 2. Layout

```
crates/slipway-core/src/{lib, version, digest, sign, path, audit, exports, system, health, schedule, progress}.rs
                        release/{mod, error, file, export, system}.rs      plan/{mod, prune}.rs
crates/slipway-root/src/{lib, settings, layout, fsutil, keys, report, helper}.rs
                        bin/{slipway_apply, slipway_rollback, slipway_health}.rs
                        apply/{mod, error, staged, unpack, step, install, exports, system, relabel, lock, open, harden, private, synced, guard}.rs
crates/slipway/src/{lib, main, config, dirs, state, clock, elevate, problem, report, health, login, bootstrap}.rs
                   fetch/{mod, feed, stage, cache, ureq_fetch}.rs
                   service/{mod, context, engine, object, notify, run, status}.rs
                   cli/{mod, args, client, wait, confirm, recover}.rs
dist/   slipway.service  org.quire.slipway.policy  org.quire.Updates1.service
scripts/install.sh  scripts/check-boundary.sh
.github/workflows/ci.yml  deny.toml
```

`slipway` re-exports `layout`, `fsutil` and `keys` from `slipway-root`, and `report` adds the logging of a
report to the type `slipway-root` defines. The three binaries are built with
`cargo build --release --locked -p slipway-root`; the user program with `-p slipway`.

Install: `scripts/install.sh` only places files. It makes folders with `install -d -m755` and files with
`install -m MODE` (beside the target, then renamed over it), sets `umask 022` for itself, and refuses to run
cargo as root: the build is a step of its own, as the person, and the root step takes `--bin-dir` (or
`--no-build`). `.github/workflows/ci.yml` runs format, clippy, the boundary check, the tests and
`cargo deny check licenses` and `advisories` (also weekly).

## 3. The release format (frozen with the CI lane)

A release is the tag `v<semver>` on the releases repository with three assets, fetched from
`<index>/latest/download/<name>` (newest) or `<index>/download/v<version>/<name>` (one version):

- `release.toml` and `release.toml.minisig` (minisign signature of exactly those bytes);
- `bundle.tar.zst`: the tree to install, paths relative to the release root.

`release.toml` holds `format = 1`, `version`, `date` (RFC 3339), `notes`, `bundle = { name, sha256,
size }`, informational `[components]`, one `[[file]]` (`path`, `sha256`, `mode`) per regular file in
the bundle, `[[export]]` (`link`, `target`) per link the system needs, and `[[system]]` (`path`) per
file the helper copies into a system folder. `slipway-core::Release`
validates all of it: simple paths only (no absolute, `..`, empty or backslash parts, nothing under
`.slipway/`), unique files with none inside another, modes that are plain permission bits readable
by the owner and not writable by anyone else, no setuid; export links only under the eleven allowed
directories (`ALLOWED_LINK_DIRS`, each named, no wildcards), unique, each pointing at a file the bundle lists
(not a folder); system files only at a path `ALLOWED_SYSTEM_FILES` names (a folder and an exact name or a name ending), unique, each with
its contents at `system<path>` in the bundle, and nothing else under `system/`. Limits: `release.toml`
24 MiB, 100000 files, notes 16 KiB, bundle 8 GiB, 16 GiB unpacked. Folders in the bundle are made
`0755` whatever the helper's umask. `release/release.py` in the desktop repository packs to exactly
these rules; FINDINGS.md "Release pipeline" lists where the two differ.

Trust anchor: a minisign public key, read from `/usr/share/slipway/release.pub` (the `pubkey_path`
setting names another) with the `SLIPWAY_RELEASE_PUBKEY` build-time text as the fallback when the
file does not exist. No key: nothing is installed and the person is told so. The key path is the
`pubkey_path` of the system settings (section 5).

## 4. Install layout and the switch

```
<root>/releases/<version>/            complete releases; .slipway/release.toml(+.minisig) is the verified copy
<root>/releases/<version>.partial/    an unpack in progress
<root>/current  -> releases/<version>
<root>/previous -> releases/<version>
<root>/.private/                      0700, root only: the update lock and the private copy of a bundle
```

The root defaults to `/opt/quire`. `slipway-core::plan_install` turns (current, previous, releases,
target) into ordered steps: `Unpack`, `Publish` (rename `.partial` to the version), `SetPrevious`,
`SwitchCurrent`, `SyncExports`, `SyncSystem`, `Prune`. Every prefix of the list leaves a working system: the new
tree is complete and flushed before the single `rename(2)` that changes what runs (`current.new` is
made, then renamed over `current`). A target not newer than `current` is refused; a rollback is the
same switch back with `current` and `previous` exchanged, so it can be undone. At most current,
previous and one unswitched newer release stay on disk. A step after the switch that fails (the
links or the system files cannot be updated) puts `current` and `previous` back where they were and
re-syncs for the release that is current again; the error says whether that worked. If it did not,
the daemon still writes the health marker for whatever is current (section 7).

Exports: after the switch each `[[export]]` link is made to point at `<root>/current/<target>`. A
link is created when missing, replaced only when it already points into the root, left alone and
reported when something else is there (a regular file, a link elsewhere), and removed when the
release being left listed it, the new one does not, and it still points into the root.
`slipway-core::plan_exports` decides from what the caller observed; `apply/exports.rs` observes and
acts. The export base (`/` on a real system) is a field of `Layout`, so tests use a temporary
directory.

System files: some things work only as a real root-owned file in a system folder (a udev rule, a polkit
rule, a login service, the keyboard daemon's unit). The bundle carries them under `system/<absolute
path>`, `release.toml` lists each destination in `[[system]]`, and `SyncSystem` copies the verified file
from the new release's tree (atomic rename, the listed mode). The destination must be a path `ALLOWED_SYSTEM_FILES` names:
a folder and either one exact name or any plain name with a given ending. Today that is `/etc/pam.d/sill`,
`/etc/udev/rules.d/{60-sill-ddc,70-keycap,70-palmrest}.rules`, `/etc/modules-load.d/{sill-ddc,keycap}.conf`,
`/etc/modprobe.d/99-palmrest.conf`, `/usr/lib/systemd/system/{keycap,turnstile}.service`,
`/usr/lib/systemd/system/turnstile.socket`, `/etc/udev/rules.d/72-turnstile.rules`,
`/etc/modules-load.d/turnstile.conf`, `/etc/polkit-1/rules.d/keycap.polkit.rules`, `/etc/porter/{callers,inferd-callers}.toml`,
`/etc/quire/*-callers.toml` and `/etc/xdg/quire/*.toml`; anything else makes the whole release refused. The helper is
installed by hand and not by a release, so a new system file needs a new helper (this is the price of the list): a
release that places the input service's files (`turnstile.*`, `72-turnstile.rules`) needs the helper that knows them,
so re-run the installer (`scripts/install.sh`) from the new source before updating to it; an older helper refuses
that release whole.
A file is overwritten or removed only when it still holds exactly what an earlier release placed
(`slipway-core::plan_system`, by digest); a person's own file, an edited one, or a link is left alone and
reported like a conflicting export. So is a name that is empty in `/etc` but that the distribution ships under
the same name in its own folder (`/usr/lib/pam.d`, `/usr/lib/udev/rules.d`, `/usr/lib/modules-load.d`,
`/usr/lib/modprobe.d`, `/usr/share/polkit-1/rules.d`: Fedora and Arch keep them there; `vendor_copies`): placing ours
would hide the vendor's. The desktop's `pack` step must mirror the same list and fail when `system/` holds a path
it does not name.

Telling the system (`apply/settle.rs`): after the system files are placed or removed the helper makes them take
effect, with programs found at fixed paths (never `PATH`). `commands` decides from the changed paths alone: a file in
`/usr/lib/systemd/system` gets `systemctl daemon-reload`; a file in `/etc/udev/rules.d` gets `udevadm control
--reload-rules` and `udevadm trigger --subsystem-match=misc`; the file `turnstile.socket` being *placed* (not removed)
also gets `systemctl enable --now turnstile.socket`, after the reload. A missing program is skipped and a failing one
is a warning in the report, as with the security labels: the files are in place either way. It is a second method on
the `Relabel` seam (`settle`, a no-op by default), implemented by `Settling` (the real helper's choice, wrapping
`Selinux`), so tests that use `NoRelabel` or a fake never run a real program.

SELinux: after a release is published and before it becomes current, and after system files are
placed, the helper runs `restorecon -RF <root>` and `restorecon -F <files>` (`apply/relabel.rs`), only when
`/sys/fs/selinux/enforce` exists and the tool is installed; a missing tool is not an error and a failing
one is a warning printed by the helper. The program runner is a trait (`RunProgram`) so tests use a fake.

## 5. Who runs as whom

```
person's session                          root (pkexec; one polkit action per program)
slipway run  --download, verify--> cache --> slipway-apply <staged dir>   org.quire.slipway.apply     no password
slipway rollback (Settings)  ------------> slipway-rollback               org.quire.slipway.rollback  administrator's password
slipway recover / confirm  --------------> slipway-health recover|confirmed  org.quire.slipway.health  no password
```

The three programs are separate files because polkit matches an action to the program's path: the installing
program cannot go back (a downgrade) at all, and the one that can asks for the administrator's password
(`auth_admin_keep`). Installing a newer release signed with the project's key stays passwordless. They are also
the only programs that run as root, and are built from their own crate (`slipway-root`) so that what runs with
those rights links nothing it does not need: no network client, bus or async runtime. The health
program is described in section 7.

Settings come from two files. The system file `/etc/slipway/config.toml` (written once by
`scripts/install.sh`, never overwritten by it) holds `root` and `pubkey_path` and may hold any other
setting; `slipway-apply` reads only it (and only its `root` and `pubkey_path`, `slipway_root::settings`), and the daemon and the command line read it first, so they
cannot disagree about the root or the key. The person's own `$XDG_CONFIG_HOME/slipway/config.toml`
may only set `settings_command` and `name` (what the notification's button runs and what the software
is called); every other key in it is ignored.

Downloads (`fetch/stage.rs`) resume: the bundle is written as `<name>.part`, and what a broken connection or an
earlier run left there is hashed again and continued with a range request (a site that ignores the range is
read from its start and the first bytes dropped). There is no cap on the whole download: one connection is read
for at most two minutes (`READ_WINDOW`), a connection that brought bytes is simply continued, and one that
brought none counts as a stall; four stalls in a row give up. The digest of the finished file decides, so
resuming trusts nothing. The cache keeps only the version being fetched (`prune_cache`).

The daemon never writes under the root. It downloads into `$XDG_CACHE_HOME/slipway/<version>/`,
checks the signature of `release.toml` and then the bundle's size and digest, and only then asks
`slipway-apply` to install the directory. The helper trusts nothing but the path, and not even that:

- Its first instruction sets umask 022; it then moves to `/`, switches off core dumps and lifts the
  limits the caller may have lowered (`pkexec` clears the environment, not these). Everything it
  creates also gets its mode explicitly, so a hostile umask changes nothing.
- It takes the lock in `<root>/.private` (a `0700` folder; `flock` works on a file opened for
  reading, so a lock in a folder others can enter could be held by anybody).
- Every file the caller names is opened once with `O_NOFOLLOW | O_NONBLOCK` and read from that handle
  only if it is a regular file, so a pipe or a link cannot hang or redirect the helper.
- It checks the signature with the key on this computer, then copies the bundle once into a private
  file that has no name, hashing it as it copies (at most the listed size plus one byte), and unpacks
  from that copy. Changing the staged bundle after the check gains nothing. It refuses to start when
  the file system has less than four times the bundle's size free.
- It unpacks into `<version>.partial`, which is `0700` with files `0600` until it is whole (regular
  files and plain directories only, each created fresh, nothing through a link, each file hashed as
  it is written), keeps `.slipway/release.toml` and its signature (`0644`, root-owned), makes the
  folders `0755`, flushes, then runs the plan.
- Whatever it later reads back from a release folder (the kept record, a system file's contents) is
  checked again: the record against the signature with the key, a system file against the digest in
  the signed description, from a handle the helper opened itself.

Its root and key come from `/etc/slipway/config.toml`, never from the caller. Exit codes: 0 done,
1 failed, 2 usage, 3 refused (not newer / nothing to go back to), 4 did not verify, 5 busy, 6 no key.

On success the helper prints a report on its standard output (`slipway_root::report::Report`): `current <version>`,
then one `left-alone <path>` per link or system file it left alone because something else is in the way, and one
`warning <text>` per thing that did not work but did not stop the update (the same lines also go to its error
output for a person running it by hand). The update service parses it, logs each line, and tells the person once
("A few things you had set up yourself were left as they are") when something was left alone. The service stops
waiting for the helper after 30 minutes (`HELPER_PATIENCE`), and kills what it can; a stuck helper no longer
leaves the state at "installing" for ever.

## 6. The session service

`slipway run` owns `org.quire.Updates1` at `/org/quire/Updates1` on the session bus.

Properties (all emit `PropertiesChanged`): `State` (idle, checking, available, downloading, ready,
installing, installed, failed), `Current`, `Available` ("" when none), `Notes`, `Size` (bytes),
`Progress` (permille), `LastChecked` (unix seconds, 0 never), `Problem` (plain words), `ProblemKind` (the stable
slug of the failure, for programs only and never shown: `no-key`, `key-unreadable`, `offline`, `unverified`,
`not-allowed`, `busy`, `nothing-to-install`, `nothing-to-restore`, `not-a-version`, `version-changed`,
`install-not-from-here`, `rollback-not-from-here`, `caller-unknown`, `other`; "" when there is no problem; it changes
with `Problem`, set and cleared together; `Problem::kind` in `problem.rs`; a program that finds a slug it does not
know treats it as a plain failure).
Methods: `Check()`, `Install(version: s)`, `Rollback()`. Who is calling is decided by peephole (`service/gate.rs`):
the bus names the caller's process, peephole holds it by the descriptor the bus daemon gave, reads its image and its
unit and judges it against the desktop's own table (`/etc/quire/desktop-callers.toml`, root-owned; loaded once at
start by `bootstrap.rs`, an empty table when it is missing or not trustworthy, which refuses every `Install` and
`Rollback`). `Install` is answered for the roles `shell` (proved up to the unit manager), `settings` and
`updater_cli` (the `slipway` program itself; both known by their root-owned image); `Rollback` for `shell` and
`settings`, and the system then asks for the administrator's password; `Check` for any caller that can be
identified at all. A refused `Rollback` or `Check` is `org.freedesktop.DBus.Error.AccessDenied` with a plain
sentence; a refused `Install` is `org.quire.Updates1.Error.AccessDenied` with its own (all in `problem.rs`).
The properties are served by the bus library to any caller on the person's session bus and are not gated: they hold
the version numbers, the notes and a state word. The gate keeps a stray program from starting an update nobody clicked
or from putting a password prompt in front of the person; it is not the boundary for a downgrade (the password is: a
program of the person can run the genuine image of a listed program with a preloaded library). `Install(version)`
can in any case only install a newer release signed by the project.
The caller names the
version it showed the person (a semver string of at most `MAX_VERSION_BYTES` = 64 bytes, `service/wanted.rs`); the
service installs only when the version it would install is that one, so a release that arrived between showing
and pressing is never installed unseen. A mismatch (or no update on offer) is refused with the error
`org.quire.Updates1.Error.VersionChanged` before anything is downloaded or staged; text that is not a version
is `org.quire.Updates1.Error.NotAVersion`; anything else that stops the start (work in progress) is
`org.quire.Updates1.Error.Refused`. Each error carries a plain sentence as its message (`service/refusal.rs`).
The command line reads `Available` (checking first when it is empty) and passes that. Each returns at once, having already moved `State`
(so a caller that polls `State` never sees the old value after the call returns), and refuses with
a plain sentence when work is in progress; the work runs on a spawned task and the blocking parts
(ureq, the helper) on blocking threads.

`service/engine.rs` is the flow: check (latest `release.toml` verified, compared with the installed
version through `standing`, only `Newer` is offered), notify once per version, download with
`ProgressGate`-throttled progress, install through the `Elevate` seam, remember the install as
pending first login. The schedule (`next_check`) runs from `service/run.rs`: at start if overdue,
24 h plus up to an hour of per-machine jitter after a good check, an hour after a failed one.

State file `$XDG_STATE_HOME/slipway/state.toml` (last check and outcome, notified version, confirmed
version, pending marker, refused version) is written beside and renamed over. The configuration files
are only read.

A version that was rolled back, automatically at start-up or by `Rollback()`, is remembered as
`refused` (the newest such). `slipway_core::offer` is the rule: a found version is offered only when
it is newer than what runs and newer than the refused one, so the next check leaves `State` at `idle`
with `Available` empty until something newer is published.

The notification's "Details" button is honoured only when the `ActionInvoked` signal was sent by the
connection that owns `org.freedesktop.Notifications` (`service/notify.rs`), as for any client of that
service. `dist/org.quire.Updates1.service` lets the session bus start the service on demand: it names
`SystemdService=slipway.service`, so the bus starts the same user unit the session starts at login.

## 7. Health and automatic rollback

An install leaves the marker `pending { version, stage = awaiting_first_login, session }`, where
`session` is the login the install ran in. A login is the boot id (`/proc/sys/kernel/random/boot_id`,
read through the `login::here` seam) and `XDG_SESSION_ID` when the session manager set one.

**What the session must call** (the desktop's `desktop-session`, or the shell). Both work when the update
service is not running and need no session bus:

- `slipway recover`, once per session start, **before the compositor is started**. It asks the privileged
  health helper (`slipway-health recover`, a separate polkit action that needs no password) to note that
  this boot started the current release without anyone having vouched for it, and to put the previous
  release back when that has happened in more than two *different* boots (`MAX_UNCONFIRMED_BOOTS`). Exit 0:
  nothing to do; exit 5: the previous version was put back, start the session again; exit 1: could not
  ask (log it, start anyway).
- `slipway confirm`, once per login, **after the shell has answered**. It records the current release as
  good in the person's state (so the service stops watching it) and in the helper's record (so the release is
  never rolled back for the boots it used while being tried). Running it again changes nothing; with no
  release installed under the root it does nothing and exits 0; exit 1 means the system could not be told.

Why the helper and not the person's state decides: a rollback without a password must not be something a
program of the person can ask for. The record is in `<root>/.private/health.toml` (root only), counts only
the boot id the helper reads itself, and a start in a boot already counted changes nothing; a release that
was confirmed, or that was just put back, is never rolled back. The cost: a crash loop inside one boot
(login screen, log in, crash, again) is not counted; it takes restarts of the computer. Going back by hand
(section 6 `Rollback()`, or `slipway-rollback` run by an administrator) is for that case.

At every start of the service `health::on_start` also applies `slipway-core::decide_boot` with the login of
this start:

| marker | confirmed | this start | action |
| --- | --- | --- | --- |
| none | | | nothing |
| for another version, or confirmed | | | clear it |
| set in this very login (a daemon restart) | no | | keep waiting, change nothing |
| awaiting first login | no | another login | move to first login seen, recording this login |
| first login seen | no, previous exists | another login or boot | ask the health helper to recover; if it restored, clear the marker, remember the version as refused, notify "The update could not start, so the previous version was restored."; if it declined, change nothing |
| first login seen | no, nothing to restore | another login or boot | clear it |

A failed recovery marks the version refused and leaves the marker, so the next start asks again.
"The same login" means the same known boot id and the same session id (neither having one counts
as equal). An unknown boot id, or a marker written before the login was recorded, never counts as
the same, which is the earlier rule. With no `XDG_SESSION_ID` at all, a second login on the same boot
is indistinguishable from a restart, so the check waits for the next boot.

## 8. The seams

| Trait | Real | Test |
| --- | --- | --- |
| `fetch::Fetch` | `UreqFetch` (HTTPS only) | in-memory map (`tests/it/feed_stage.rs`) |
| `elevate::Elevate` | `Pkexec` (three programs) | recording stand-ins (`tests/it/health_start.rs`, `session_guard.rs`); `sh` scripts for `Pkexec` itself (`elevate_run.rs`) |
| `slipway_core::Authenticator` | `PublicKey` (minisign) | accept-all and reject-all (`tests/it/support.rs`) |
| `Layout { root, export_base }` | `/opt/quire` and `/` | a temporary directory for both |
| `login::here(var, read)` | `login::current()`: the environment and `/proc` | closures |

The D-Bus object and the notification code are not covered by the unit tests (no bus in a test);
they are checked by the orchestrator's session scenarios.
