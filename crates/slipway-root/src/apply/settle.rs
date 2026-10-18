//! Tells the system about system files the helper placed or removed, so they take effect now and not
//! at the next start.
//!
//! A unit file, a device rule and a service that should be listening are three kinds of thing the
//! system reads once. After `SyncSystem` the helper asks the system to read the unit folder again, to
//! read the device rules again and apply them to devices already there, and to start the one socket
//! the desktop's input service listens on, and the two services that map the keyboard and read the touchpad. What runs is decided by [`commands`] from the paths alone
//! (a pure function), by fixed program paths, never by `PATH`; a missing program is not an error and a
//! failing one is a warning, like the security labels: the files are in place either way.

use super::relabel::{Ran, RunProgram};
use std::ffi::OsString;
use std::path::Path;

/// The folder of the unit files the helper may place.
const UNIT_FOLDER: &str = "/usr/lib/systemd/system/";
/// The folder of the device rules the helper may place.
const RULE_FOLDER: &str = "/etc/udev/rules.d/";
/// The one socket that is started as soon as it is placed: the input service waits behind it.
const INPUT_SOCKET: &str = "/usr/lib/systemd/system/turnstile.socket";

/// The one service that is enabled and started as soon as it is placed: the keyboard map. It is placed in the
/// vendor unit folder and its `[Install]` section is never acted on by anything else, so without this a
/// release-only install would leave the keys unmapped until someone enabled it. Its unit name is the file name.
const KEYMAP_SERVICE: &str = "/usr/lib/systemd/system/keycap.service";

/// The other service that is enabled and started as soon as it is placed: the touchpad daemon (Magic Mouse
/// gestures). Like the keyboard map it is a system service whose `[Install]` section (`multi-user.target`) nothing
/// else acts on, so a release-only install would leave it stopped. Its unit name is the file name.
const TOUCHPAD_SERVICE: &str = "/usr/lib/systemd/system/palmrest.service";

/// Where the service manager's tool is looked for (a fixed list, so root never depends on `PATH`).
const MANAGER_TOOLS: &[&str] = &["/usr/bin/systemctl", "/bin/systemctl"];
/// Where the device manager's tool is looked for.
const DEVICE_TOOLS: &[&str] = &[
    "/usr/bin/udevadm",
    "/usr/sbin/udevadm",
    "/sbin/udevadm",
    "/bin/udevadm",
];

/// One system file the helper placed or removed, by its absolute destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Written (new, or replacing one an earlier release placed).
    Placed(String),
    /// Removed because the release left no longer lists it.
    Removed(String),
}

impl Change {
    fn path(&self) -> &str {
        match self {
            Self::Placed(path) | Self::Removed(path) => path,
        }
    }
}

/// A program to run with fixed arguments: the first of `tools` that is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// Where the program may be.
    pub tools: &'static [&'static str],
    /// Its arguments.
    pub args: &'static [&'static str],
}

const READ_UNITS_AGAIN: Command = Command {
    tools: MANAGER_TOOLS,
    args: &["daemon-reload"],
};
const READ_RULES_AGAIN: Command = Command {
    tools: DEVICE_TOOLS,
    args: &["control", "--reload-rules"],
};
const APPLY_RULES: Command = Command {
    tools: DEVICE_TOOLS,
    args: &["trigger", "--subsystem-match=misc"],
};
const START_INPUT_SOCKET: Command = Command {
    tools: MANAGER_TOOLS,
    args: &["enable", "--now", "turnstile.socket"],
};

const START_KEYMAP_SERVICE: Command = Command {
    tools: MANAGER_TOOLS,
    args: &["enable", "--now", "keycap.service"],
};
const START_TOUCHPAD_SERVICE: Command = Command {
    tools: MANAGER_TOOLS,
    args: &["enable", "--now", "palmrest.service"],
};

/// What to run after `changes`, in order: units are read again before the socket is started, rules are
/// read again before they are applied, and the input socket is started before the keyboard map, which asks
/// it for its virtual keyboards, and the touchpad daemon last.
pub fn commands(changes: &[Change]) -> Vec<Command> {
    let touches = |folder: &str| changes.iter().any(|c| c.path().starts_with(folder));
    let mut out = Vec::new();
    if touches(UNIT_FOLDER) {
        out.push(READ_UNITS_AGAIN);
    }
    if touches(RULE_FOLDER) {
        out.push(READ_RULES_AGAIN);
        out.push(APPLY_RULES);
    }
    let placed = |wanted: &str| {
        changes
            .iter()
            .any(|c| matches!(c, Change::Placed(path) if path == wanted))
    };
    if placed(INPUT_SOCKET) {
        out.push(START_INPUT_SOCKET);
    }
    if placed(KEYMAP_SERVICE) {
        out.push(START_KEYMAP_SERVICE);
    }
    if placed(TOUCHPAD_SERVICE) {
        out.push(START_TOUCHPAD_SERVICE);
    }
    out
}

/// Runs `command` with the first of its tools that exists.
fn run_first(runner: &impl RunProgram, command: Command) -> Ran {
    let args: Vec<OsString> = command.args.iter().map(OsString::from).collect();
    for tool in command.tools {
        match runner.run(Path::new(tool), &args) {
            Ran::Missing => {}
            done => return done,
        }
    }
    Ran::Missing
}

/// Tells the system about `changes` with the programs `runner` runs; a plain note for each thing that
/// did not work.
pub(super) fn settle(runner: &impl RunProgram, changes: &[Change]) -> Vec<String> {
    commands(changes)
        .into_iter()
        .filter_map(|command| match run_first(runner, command) {
            Ran::Failed(why) => Some(format!(
                "the system could not be told about the new system files: {why}"
            )),
            Ran::Missing | Ran::Succeeded => None,
        })
        .collect()
}
