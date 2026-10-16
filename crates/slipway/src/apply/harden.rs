//! The helper's own process state, reset before it does anything else.
//!
//! `pkexec` clears the environment but not the umask, the resource limits or the process's place in the
//! file system: they come from whoever ran it. A umask of 0 would make everything the helper creates
//! writable by everybody until it is fixed up; a tiny file-size limit would make an update fail half way.

use rustix::fs::Mode;
use rustix::process::{Resource, Rlimit, chdir, setrlimit, umask};
use std::io;

/// The files and folders the helper creates are never writable by group or others.
pub fn set_umask() {
    let _ = umask(Mode::WGRP | Mode::WOTH);
}

/// Sets the umask first, then moves to `/`, switches off core dumps (a dump of the helper holds the
/// release key and the files it reads) and lifts the limits the caller may have lowered.
pub fn reset_process() -> io::Result<()> {
    set_umask();
    chdir("/")?;
    let none = Rlimit {
        current: Some(0),
        maximum: Some(0),
    };
    let unlimited = Rlimit {
        current: None,
        maximum: None,
    };
    for (resource, limit) in [
        (Resource::Core, none),
        (Resource::Fsize, unlimited),
        (
            Resource::Nofile,
            Rlimit {
                current: Some(4096),
                maximum: Some(4096),
            },
        ),
    ] {
        // Best effort: a limit that cannot be set does not stop an update.
        let _ = setrlimit(resource, limit);
    }
    Ok(())
}
