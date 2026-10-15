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
9. **The privileged helper takes its root and key from `/etc/slipway/config.toml`.** The daemon's
   `root` setting must agree with it; on a default install both are `/opt/quire`.
10. **Zero-length and over-large bundles** (0, or over 8 GiB), notes over 16 KiB, `release.toml` over 1 MiB,
    more than 100000 files and more than 16 GiB unpacked are refused outright.
11. **Notification senders are not checked.** The "Details" action is matched by notification id from the
    notification service's `ActionInvoked` signal without confirming the sender is the server's owner (sill's
    own code does). Any process of the same user could therefore open the Software Update page by sending a
    forged signal for one of our ids; it can do nothing else. Closes when the sender check is copied in.

## Open questions

1. **A daemon restart in the installing session counts as the "first login".** The marker moves on at the
   first start after an install; if the daemon is restarted (crashed, session restarted) before the person
   logs out, the next start sees "first login seen" without a confirmation and rolls back although the old
   session was fine. A boot-id or session-id in the marker would settle it. The rule as specified is
   implemented.
2. **A rolled-back version is offered again** by the next check (it is newer than what is running); the
   notification is not repeated (once per version) but the Software Update page shows it. Remembering a
   rejected version in the state file is a small change if wanted.
3. **`slipway confirm` writes the state file directly**, not through the daemon, so it works when the
   daemon is down; a read-modify-write race with the daemon's own writes is possible but both write
   whole-file renames and the fields are disjoint in practice.
4. **Who writes `/etc/slipway/config.toml`?** Nothing ships it; defaults match the default layout. The
   desktop installer should write it if the root moves.
5. **D-Bus activation file.** None is shipped; the user unit (part of the graphical session) starts the
   daemon, and the CLI says "The update service is not running." otherwise.
6. **CONVENTIONS.md still names transom** (it was copied); the rules apply unchanged.

## Not verified (no compiler in this lane)

See the lane report: the whole tree is unbuilt. Every API was looked up in the registry sources, but
nothing has been compiled or run.
