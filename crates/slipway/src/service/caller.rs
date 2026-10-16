//! Who is allowed to ask the update service to go back to an earlier version.
//!
//! Going back is a downgrade, so the system asks for the administrator's password before it happens
//! (the rollback helper has its own polkit action). The service adds a second, plain check in front of
//! that: the process that called must belong to the desktop's own shell or to the Settings app, which
//! is where the page that offers it lives. The bus names the caller's process, `/proc/<pid>/cgroup`
//! names the unit or app scope it runs in, and the settings file lists who counts. This stops a stray
//! program from putting a password prompt in front of the person; it is not a boundary on its own, since
//! a program of the person can start itself in a scope of its choosing (the password is the boundary).

use std::path::Path;
use zbus::Connection;
use zbus::fdo;
use zbus::message::Header;
use zbus::names::BusName;

/// What a refused caller is told.
const NOT_FROM_SETTINGS: &str = "Going back to an earlier version is done from Settings, where you will be asked for your password.";

/// The unit or scope a process runs in: the last part of the cgroup path on the line of the unified
/// hierarchy (`0::/user.slice/.../app-org.quire.Settings-1234.scope`). `None` when there is no such line.
pub fn leaf_of(cgroup: &str) -> Option<&str> {
    let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|leaf| !leaf.is_empty())
}

/// The app part of an app scope's name: `app-[<launcher>-]<id>-<number>.scope` gives `[<launcher>-]<id>`.
fn app_part(leaf: &str) -> Option<&str> {
    let inner = leaf.strip_prefix("app-")?.strip_suffix(".scope")?;
    let (rest, number) = inner.rsplit_once('-')?;
    (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then_some(rest)
}

/// Whether the process in `leaf` counts as one of `allowed`: a unit by its exact name (`sill.service`),
/// an app by its id (`org.quire.Settings`, matching the scope its launcher put it in).
pub fn is_allowed(leaf: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|entry| {
        leaf == entry
            || app_part(leaf).is_some_and(|app| app == entry || app.ends_with(&format!("-{entry}")))
    })
}

/// Lets the call through only when its sender is one of `allowed`; `proc_root` is `/proc` (a parameter
/// so a test can read a fixture tree).
pub async fn check(
    connection: &Connection,
    header: &Header<'_>,
    allowed: &[String],
    proc_root: &Path,
) -> fdo::Result<()> {
    let denied = || fdo::Error::AccessDenied(NOT_FROM_SETTINGS.to_owned());
    let sender = header.sender().ok_or_else(denied)?;
    let name = BusName::try_from(sender.as_str()).map_err(|_| denied())?;
    let pid = fdo::DBusProxy::new(connection)
        .await?
        .get_connection_credentials(name)
        .await
        .map_err(|_| denied())?
        .process_id()
        .ok_or_else(denied)?;
    let cgroup = tokio::fs::read_to_string(proc_root.join(pid.to_string()).join("cgroup"))
        .await
        .map_err(|_| denied())?;
    match leaf_of(&cgroup) {
        Some(leaf) if is_allowed(leaf, allowed) => Ok(()),
        Some(_) | None => Err(denied()),
    }
}
