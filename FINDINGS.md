# Findings

Departures from the plan, standing facts and open items. An entry names the condition that closes it.

## Decisions and departures

1. **Polkit: installing is allow_active without a password; going back asks for one (owner, 2026-10-16).**
   `org.quire.slipway.apply` has `allow_active=yes`, `allow_inactive=no`, `allow_any=no`. That is safe only because
   `slipway-apply` installs nothing that does not verify against the release key on the computer and refuses
   anything not newer than what runs; it cannot be pointed at another root or key (it reads
   `/etc/slipway/config.toml`, not the caller's configuration) and, since decision 23, it cannot go back at all.
   Going back is `slipway-rollback` under `org.quire.slipway.rollback` (`auth_admin_keep`). The health program
   `slipway-health` has `org.quire.slipway.health` (`yes`), see decision 22. Closes if the owner wants a password
   for installs: change `allow_active` of the first action to `auth_admin_keep`.
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
    unpack makes, and `<root>` and `<root>/releases` when they are new, are now set to `0755`. Since
    decision 18 the helper also sets its own umask, and a test runs the install under umask 0 and 077.
14. **An export points at a file.** A link to a folder in the bundle was accepted; the pipeline never makes
    one and a folder link is not needed, so it is refused (`ExportTargetMissing`).

15. **More export folders (lane distro-slip, 2026-10-16).** `ALLOWED_LINK_DIRS` gained `/usr/share/quire`,
    `/usr/share/porter`, `/usr/share/stoker` and `/usr/share/fonts`, so intents, skills, settings schemas,
    providers, the model catalog and the bundled colour-emoji font reach the places their readers look.
    The user-level systemd and D-Bus service folders were already listed. Each folder is named; a table test
    covers accept and refuse.
16. **System files (lane distro-slip).** `[[system]]` in `release.toml` and `SyncSystem` (see ARCHITECTURE
    "System files"). Open question 5 is closed (ROOT-07).
17. **SELinux labels (lane distro-slip).** `slipway-apply` runs `restorecon` on the root and on placed
    system files where SELinux is in use; `scripts/install.sh` adds the `semanage fcontext` rules.
    Not run on a real SELinux computer: the first Fedora install should check `ls -Z /opt/quire/current/bin`
    and that `keycap.service` starts under the enforcing policy.

18. **The helper does not trust what the caller can touch (audit ROOT-01, ROOT-05, ROOT-06).** It sets
    umask 022 first (via `rustix`, no `unsafe`); makes the new tree `0700`/`0600` until whole; keeps
    `.slipway/release.toml` root-owned `0644`; checks the kept record's signature again whenever it reads
    it (`recorded()`), so an on-disk record is trusted only as far as the key vouches for it; re-hashes a
    system file against the signed digest at the moment it is copied; keeps the lock in `<root>/.private`;
    opens named files without following links or waiting on pipes; and copies the bundle once into an
    unnamed private file while hashing it. A rollback now needs the release key too (it verifies the kept
    record of the release it returns to before moving anything). `StagedError::BundleSize` no longer
    says how big the named file was (it leaked the size of any file root can read).
    Deferred: a per-file `size` in `[[file]]` (ROOT-06's other half) changes the frozen release format and
    needs `release.py` and `pack.sh` to follow; with the digest taken on a private copy the stream can no
    longer be swapped for a different one, so the remaining risk is a signed bundle that is itself huge.
19. **A step after the switch that fails puts the earlier release back (ROOT-11).** See ARCHITECTURE s.4. If the
    helper could not even do that, the service still writes the health marker for whatever is current, so the
    release is watched and rolled back at the next login instead of running unmonitored. A failed automatic
    rollback marks the release refused (the marker stays, so the next start tries again).
20. **Conflicts and warnings reach the person (ROOT-12).** The helper's report is on its standard output and
    is parsed by the service (ARCHITECTURE s.5). A conflict is logged and posted once as a notification; a
    warning (a security label that could not be set) is logged only, because nothing a person can do changes it.
    `Elevate::install` and `rollback` return the report; the helper run has a 30-minute limit.
21. **Downloads resume and stall instead of timing out (ROOT-13).** The one hour cap and the restart from zero
    are gone (ARCHITECTURE s.5). `Fetch::open_from` is the seam (its default drops the first bytes, so a fake
    or a site without ranges still works). The windows (`READ_WINDOW` 120 s, `ANSWER_WITHIN` 60 s) and the
    stall allowance (`Patience`, 4) are not tuned on the slow HiNet route; they are the first thing to look at if
    a download there still fails. Not run against the real release site: whether GitHub's download redirect
    keeps the `Range` header is unchecked (if it does not, the first bytes are dropped and it still works, just
    slower).22. **The session gets two calls, and the rollback it may make needs no password (ROOT-02, ROOT-03).**
    `slipway confirm` and the new `slipway recover` are the contract (ARCHITECTURE s.7). The automatic
    rollback is decided by a root-side record counting different boots, not by the person's state file, so
    it cannot be forced by a program of the person. **Needs the owner's eye:** it is a rollback with no
    password (the other, on request, will ask for one, decision 23), bounded by "more than two different
    boots" and "only to the kept previous release". Counting boots makes a crash loop inside one boot
    invisible to it. If the owner wants a one-boot loop to recover, the desktop's crash guard has to say
    so on a channel root can trust (there is none today).23. **Going back asks for the administrator's password (ROOT-04, owner 2026-10-16).** A rollback is a downgrade
    with no signature check on the way down, so it is its own program and polkit action (ARCHITECTURE s.5), and
    the service answers `Rollback()` only for the shell's unit and the Settings app (`callers`). Not closed here:
    "a floor version a rollback may not go below" in root-owned state, which would make a security fix
    un-roll-back-able even with the password. `Install()` over the bus is not gated (the daemon's `Install()`
    starting an update nobody clicked is limited to a newer signed release, which is what the daily check would
    offer anyway). The CLI `slipway rollback` from a terminal is refused by the caller check (it runs in a
    terminal's scope); the way back there is `pkexec /usr/libexec/slipway-rollback`.

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

5. **Closed (ROOT-07, 2026-10-16): the system files are named one by one.** The folder list is gone;
   `ALLOWED_SYSTEM_FILES` lists the exact files (see ARCHITECTURE s.4). A compromised build step that adds
   `system/etc/polkit-1/rules.d/00-all.rules` now makes the release unusable instead of giving root. Left to the
   desktop: `pack` should refuse a `system/` tree that differs from the same list (exact diff in the lane report).
   Whoever adds a system file edits the list and installs a new helper by hand (decision 1 of the desktop's
   FINDINGS: a release never replaces the helper).

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
