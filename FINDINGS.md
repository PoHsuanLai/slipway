# Findings

Departures from the plan, standing facts and open items. An entry names the condition that closes it.

## Decisions and departures

1. **Polkit action is allow_active without a password.** `org.quire.slipway.apply` has
   `allow_active=yes`, `allow_inactive=no`, `allow_any=no`, as specified. This is safe only because
   `slipway-apply` installs nothing that does not verify against the release key on the computer:
   anyone at the keyboard can run it, and the only thing it can do is install or roll back to a
   project-signed release that is newer (or the kept previous one). It cannot be pointed at another root
   or key (it reads `/etc/slipway/config.toml`, not the caller's configuration). Closes if the owner
   wants a password for installs: change `allow_active` to `auth_admin_keep`.
2. **Test signature is a documentation example.** There is no way to run minisign here, so the real
   signature test (`slipway-core/tests/it/digest_sign.rs`) uses the key and prehashed signature of the
   four bytes `test` copied from minisign-verify 0.2.5's own docs (`src/lib.rs`). Install tests in
   `slipway` use an accept-all or reject-all `Authenticator` instead of signing their own
   `release.toml`; the pure signature path is exercised only through the documentation pair. Closes when a
   CI-produced `release.toml` + `.minisig` pair is added as a fixture.
3. **Methods return at once.** `Check()`, `Install()` and `Rollback()` return as soon as the work has
   started (a long download would outlast a bus call's patience); callers watch `State`. The CLI polls
   it every quarter second.
4. **"1 per 100 permille or 250 ms".** Read as: announce progress when it crosses a 100-permille step,
   when 250 ms have passed since the last announcement and the value changed, and always at 1000
   (`ProgressGate`).
5. **No release yet is not an error.** A 404 for `latest/download/release.toml` (no release published)
   counts as a successful check with nothing newer.
6. **Export targets must be shipped.** An `[[export]]` whose `target` is neither a listed file nor a folder
   holding listed files is refused when the release is parsed; a dangling link is a defect. The CI lane
   must not export a path it does not put in the bundle.
7. **`.slipway/` is reserved.** The verified `release.toml` and signature are kept at
   `<release>/.slipway/` and bundle paths under it are refused.
8. **Stale exports come from the kept record**, not from scanning the system: the links to drop are those
   in the `release.toml` kept inside the release being left, and only when they still point into the root.
9. **One system settings file decides the root and the key.** The privileged helper reads only
   `/etc/slipway/config.toml`; the daemon and the command line read it first and then the person's file,
   which may only set `settings_command` and `name` (other keys there are ignored, silently). So the two
   cannot disagree. `scripts/install.sh` writes the file once (`root`, `pubkey_path`) and never overwrites
   one; it says so when the existing file names another root than `--root`.
10. **Zero-length and over-large bundles** (0, or over 8 GiB), notes over 16 KiB, `release.toml` over 24 MiB
    (it was 1 MiB, which allowed only about 5000 of the 100000 files the file limit promises), more than
    100000 files and more than 16 GiB unpacked are refused outright.
11. **Notification senders are checked.** The "Details" action is honoured only when the `ActionInvoked`
    signal came from the connection that owns `org.freedesktop.Notifications` (the pattern of sill's account
    notices). The owner is looked up per pressed button of one of our notifications, not per signal.
12. **A rolled-back version is refused until a newer one is published.** After an automatic or a manual
    rollback the next checks leave `State` at `idle` and `Available` empty for that version and anything
    older (`slipway_core::offer`). The person cannot re-install the refused version from the page; the way
    back to it is a newer release, or deleting `refused` from the state file. Closes if the owner wants a
    "try again" button.
13. **Bundle folders are `0755`.** The tar's folder modes were ignored and folders got the helper's umask,
    so a restrictive umask under pkexec would have made `/opt/quire` unreadable to the person. Folders the
    unpack makes, and `<root>` and `<root>/releases` when they are new, are now set to `0755`. The effect of
    a restrictive umask itself is not tested (setting one needs `unsafe`); the tests check the resulting modes.
14. **An export points at a file.** A link to a folder in the bundle was accepted; the pipeline never makes
    one and a folder link is not needed, so it is refused (`ExportTargetMissing`).

15. **More export folders (lane distro-slip, 2026-10-16).** `ALLOWED_LINK_DIRS` gained `/usr/share/quire`,
    `/usr/share/porter`, `/usr/share/stoker` and `/usr/share/fonts`, so intents, skills, settings schemas,
    providers, the model catalog and the bundled colour-emoji font reach the places their readers look.
    The user-level systemd and D-Bus service folders were already listed. Each folder is named; a table test
    covers accept and refuse.
16. **System files (lane distro-slip).** `[[system]]` in `release.toml` and `SyncSystem` (see ARCHITECTURE
    "System files"). Open question 5 below is the owner's.
17. **SELinux labels (lane distro-slip).** `slipway-apply` runs `restorecon` on the root and on placed
    system files where SELinux is in use; `scripts/install.sh` adds the `semanage fcontext` rules.
    Not run on a real SELinux computer: the first Fedora install should check `ls -Z /opt/quire/current/bin`
    and that `keycap.service` starts under the enforcing policy.

## Open questions

1. **`slipway confirm` writes the state file directly**, not through the daemon, so it works when the
   daemon is down; a read-modify-write race with the daemon's own writes is possible but both write
   whole-file renames and the fields are disjoint in practice.
2. **No `XDG_SESSION_ID`, second login on the same boot.** Indistinguishable from a daemon restart, so a
   release that never confirmed is rolled back at the next boot instead of the next login. Closes if a
   session manager without that variable matters; a per-login runtime-dir marker would settle it.
3. **The first install has no slipway.** `install.sh` installs the helper, the policy, the unit, the bus
   file and the settings, but the user unit and the bus file start `<root>/current/bin/slipway`, which
   exists only after the first release is installed, and `slipway install` needs the running service.
   The desktop README ("First install on a machine") says `sudo ./dist/install.sh ... && slipway install`;
   neither works as written (the script is `scripts/install.sh`, and there is no `slipway` binary yet).
   Closes when the first-install path is decided: `install.sh --with-cli` copying a built `slipway` to
   `PREFIX/bin`, or a `slipway-apply <staged dir>` run by hand with a release downloaded by `curl`.
4. **A root other than `/opt/quire`.** `install.sh --root` retargets slipway's own unit and bus file, but the
   units and files inside the bundle name `/opt/quire/current/...` (the pipeline hard-codes `PREFIX`), so a
   different root only works for a bundle built for it.

5. **Does the release write `/etc/pam.d` at all?** The system step lists `/etc/pam.d`, `/usr/lib/systemd/system`,
   `/etc/udev/rules.d` and `/etc/polkit-1/rules.d`, each of which gives whoever holds the signing key root
   (a PAM stack can accept any password, a rule or unit can run a command as root). The signing key already
   decides what runs in the session; this widens it to the whole computer. If that is too much, delete the
   entry from `ALLOWED_SYSTEM_DIRS` and the desktop's PAM service stays an install-time step.

## Release pipeline

`desktop/release/release.py` (lane `wh-desk-release`, not landed when this was written) against
`slipway-core`'s validation. Slipway's side was fixed where slipway was wrong; the pipeline's side is
written as the exact change, for that lane (nothing of its was edited).

Agree already (checked line by line): the three asset names, `format = 1`, `[[file]]` fields, `mode = 0o...`
integers, the export directories (the same list on both sides; now eleven, see decision 15), absolute clean export links, `target`
relative to the current release, no links or devices in the bundle, folders only where files live (the
pipeline emits only the ancestors of files, slipway refuses any other folder), unique entries, bundle
digest and size, `components` as a string table, build metadata refused on both sides.

Slipway was wrong (fixed here):

- **Folder export targets.** Slipway accepted a folder holding listed files as an export target; the
  pipeline requires `target in files`. Slipway now requires a listed file.
- **`release.toml` size limit.** 1 MiB admitted only about 5000 files although 100000 are allowed. Now 24 MiB.
- **Folder modes.** The pipeline writes folders `0755` in the tar; slipway ignored that and used the umask.
  Now `0755`, see Decision 13.

The pipeline is wrong or looser than slipway (slipway will refuse what the pipeline passes); the exact
changes for `release.py`:

1. **`.slipway/` is reserved.** `clean_path` accepts a first part of `.slipway`; slipway refuses it (the
   verified `release.toml` is kept there). In `clean_path`, after the loop, add
   `if path.split("/")[0] == ".slipway": raise Refused(f"{what} {path!r}: .slipway is reserved")`.
   Applies to staged files, the manifest's files, and export targets (they all go through `clean_path`).
2. **Modes.** `check_manifest` accepts any mode up to `0o777`; slipway refuses a mode writable by group or
   others or not readable by the owner. Replace the mode condition with
   `not isinstance(f.get("mode"), int) or f["mode"] & ~0o777 or f["mode"] & 0o022 or not f["mode"] & 0o400`.
   (`pack` itself only writes 0644/0755, so this only matters to `verify`.)
3. **Version.** `SEMVER` accepts leading zeros (`01.2.3`) and empty or leading-zero prerelease parts
   (`1.0.0-a..b`, `1.0.0-01`) that slipway's parser refuses. Use the semver.org expression without build
   metadata: `^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-((0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(\.(0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?$`.
4. **Date.** `DATE` checks the shape only; slipway parses RFC 3339, so `2026-13-45T25:61:61Z` passes the
   pipeline and fails slipway. In `check_manifest` add
   `datetime.datetime.strptime(doc["date"], "%Y-%m-%dT%H:%M:%SZ")` inside a `try/except ValueError` that raises
   `Refused("date is not a real time")`.
5. **Limits.** Slipway refuses a bundle of 0 bytes or over 8 GiB (`8 << 30`), more than 100000 files,
   notes over 16 KiB (`16 * 1024` bytes of UTF-8), a `release.toml` over 24 MiB (`24 << 20`) and more than
   16 GiB (`16 << 30`) of files in total. Check all five in `pack` (after the text is built) and in `verify`
   (`bundle["size"]`, `len(files)`, `len(notes.encode())`, the file size of `release.toml`, the sum of
   `m.size` in `check_bundle`).
6. **Raw DEL in strings.** `toml_str` is `json.dumps(..., ensure_ascii=False)`, which leaves U+007F as is;
   TOML forbids it in a basic string, so notes containing it sign fine and then fail to parse in slipway.
   Return `json.dumps(text, ensure_ascii=False).replace("\x7f", "\\u007f")`.
7. **Signature kind.** Slipway verifies with legacy signatures disallowed (the prehashed `ED` kind only).
   Current minisign signs prehashed by default; an old one signs legacy unless told otherwise, and
   `minisign -V` in `check_signature` accepts both, so CI would pass what slipway refuses. After
   `minisign -V` succeeds in `check_signature`, read the second line of `release.toml.minisig`, base64-decode
   it, and `raise Refused("release.toml.minisig: not a prehashed signature")` unless it starts with `b"ED"`.
   (Not run here: no minisign on this computer.)
8. **README.** "First install on a machine" says `./dist/install.sh --pubkey`; the script is
   `scripts/install.sh` with `--root`, `--pubkey`, `--prefix`, `--destdir`, `--sysconfdir` (see Open
   question 3 for the part that does not work yet). Change `cd slipway && sudo ./dist/install.sh` to
   `cd slipway && sudo scripts/install.sh --pubkey /path/to/release.pub`.

Notes for the pipeline lane that are not mismatches:

- `build.sh` puts `slipway-apply` into the bundle at `libexec/slipway-apply` and slipway's `dist/` under
  `share/slipway/dist/`. Nothing uses either: the helper that polkit names is `/usr/libexec/slipway-apply`
  from `install.sh`, deliberately not updated by a release (a release would replace a root-run binary).
  Keep `share/slipway/dist/` out of the auto-exported directories, as now: slipway's own unit and bus file are
  installed by `install.sh`, and an export onto the same path would be reported as a conflict and left alone.
- Everything in `AUTO_EXPORT_DIRS` is exported as a link `/usr/<rel>` into `/opt/quire/current/<rel>`. systemd
  and the bus follow such links; polkit's action directory is the one to confirm on a real machine.

## Not verified (no compiler in this lane)

Nothing from the second slipway lane was compiled or run (only `cargo fmt -p` and the shell parts of
`scripts/install.sh` under a temporary `--destdir`). Guesses to check first when building:

- `zbus` 5.19: `fdo::DBusProxy::new(&Connection)`, `get_name_owner(BusName)`, `Message::header().sender()`
  with `.as_str()` (all copied from sill's `accounts/backend/actions.rs`, same zbus version).
- `Config::with_user` moves two fields out of `self` and then uses `..self`.
- `Accept` coerces to `Arc<dyn Authenticator + Send + Sync>` inside `Ok(...)` in the engine test.
- The engine test runs `Engine::begin` on a current-thread runtime and polls `snapshot()`.
- The notification-sender check and the bus activation file are not covered by tests (no bus in a test);
  the orchestrator's session scenarios check them.
