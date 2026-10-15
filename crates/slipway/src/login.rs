//! Which login this process belongs to, for the health marker.

use slipway_core::LoginSession;
use std::ffi::OsString;
use std::io;
use std::path::Path;

/// Where the kernel names this boot of the computer.
pub const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
/// The variable the session manager sets to the login session's number.
pub const SESSION_VARIABLE: &str = "XDG_SESSION_ID";

/// Reads the login: `var` looks up an environment variable and `read` reads a text file (the seams,
/// so tests need neither the real environment nor `/proc`). A part that is missing, unreadable or
/// blank is left unknown.
pub fn here(
    var: impl Fn(&str) -> Option<OsString>,
    read: impl Fn(&Path) -> io::Result<String>,
) -> LoginSession {
    let plain = |text: String| {
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    };
    LoginSession {
        boot: read(Path::new(BOOT_ID_PATH)).ok().and_then(plain),
        session: var(SESSION_VARIABLE)
            .and_then(|v| v.into_string().ok())
            .and_then(plain),
    }
}

/// The login of this process on the real system.
pub fn current() -> LoginSession {
    here(|name| std::env::var_os(name), std::fs::read_to_string)
}
