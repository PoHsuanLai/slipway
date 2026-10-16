//! The privileged programs' `main` functions, so each binary is one line and the pieces can be read
//! (and the parts that are pure tested) as a library.
//!
//! Every program does the same first thing: reset the process state it inherited from whoever ran it
//! ([`crate::apply::harden`]), before anything is created or read. The root, the key and the helper's
//! settings come from `/etc/slipway/config.toml` only, never from the caller.

use crate::apply::harden::reset_process;
use crate::apply::{
    Applied, ApplyError, ExitCode as Code, Settling, Started, install_with, note_confirmed,
    note_start, rollback_with,
};
use crate::keys::load_key;
use crate::layout::Layout;
use crate::report::Report;
use crate::settings::{RootSettings, SYSTEM_CONFIG_PATH};
use slipway_core::PublicKey;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Where the kernel names this boot of the computer.
pub const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";

const INSTALL_USAGE: &str = "usage: slipway-apply <staged folder>

Installs the signed release staged in the folder. It cannot go back to an earlier release: that is
slipway-rollback, which asks for the administrator's password.

exit codes: 0 done, 1 failed, 2 wrong command, 3 refused (not newer), 4 the release did not check out,
            5 another update is running, 6 no key to check with";

const ROLLBACK_USAGE: &str = "usage: slipway-rollback

Goes back to the previous release; the release left becomes the one a second run goes back to. The
system asks for the administrator's password before this runs.

exit codes: 0 done, 1 failed, 2 wrong command, 3 refused (nothing to go back to), 4 the kept record of
            the release did not check out, 5 another update is running, 6 no key to check with";

const HEALTH_USAGE: &str = "usage: slipway-health recover
       slipway-health confirmed

recover    note that the session is about to start the current release; if it has failed to start in
           too many boots, put the previous release back
confirmed  note that the session started well on the current release

exit codes: 0 done, 1 failed, 2 wrong command, 5 another update is running, 6 no key to check with,
            7 the previous release was put back";

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed<T> {
    /// Print the help text.
    Help,
    /// The words were not understood.
    Bad,
    /// Do this.
    Run(T),
}

/// What `slipway-health` can be asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthRequest {
    /// A start of the session is about to happen.
    Recover,
    /// The session started well.
    Confirmed,
}

/// Reads the arguments of `slipway-apply`: the folder a release is staged in.
pub fn parse_install(mut args: impl Iterator<Item = String>) -> Parsed<PathBuf> {
    let Some(first) = args.next() else {
        return Parsed::Bad;
    };
    if args.next().is_some() {
        return Parsed::Bad;
    }
    match first.as_str() {
        "--help" | "-h" => Parsed::Help,
        s if s.starts_with('-') => Parsed::Bad,
        _ => Parsed::Run(PathBuf::from(first)),
    }
}

/// Reads the arguments of `slipway-rollback`: there are none.
pub fn parse_rollback(mut args: impl Iterator<Item = String>) -> Parsed<()> {
    match (args.next().as_deref(), args.next()) {
        (None, _) => Parsed::Run(()),
        (Some("--help" | "-h"), None) => Parsed::Help,
        _ => Parsed::Bad,
    }
}

/// Reads the arguments of `slipway-health`.
pub fn parse_health(mut args: impl Iterator<Item = String>) -> Parsed<HealthRequest> {
    let Some(first) = args.next() else {
        return Parsed::Bad;
    };
    if args.next().is_some() {
        return Parsed::Bad;
    }
    match first.as_str() {
        "--help" | "-h" => Parsed::Help,
        "recover" => Parsed::Run(HealthRequest::Recover),
        "confirmed" => Parsed::Run(HealthRequest::Confirmed),
        _ => Parsed::Bad,
    }
}

fn finish(code: Code) -> ExitCode {
    ExitCode::from(code as u8)
}

/// What a privileged program reads before it acts, all from the system settings.
struct Prepared {
    layout: Layout,
    key: PublicKey,
}

/// Resets the process, then reads the system settings and the key. `Err` carries the exit code, after
/// the reason was printed.
fn prepare(program: &str) -> Result<Prepared, Code> {
    let settings = RootSettings::load(Path::new(SYSTEM_CONFIG_PATH)).map_err(|e| {
        eprintln!("{program}: {e}");
        Code::Failed
    })?;
    let key = load_key(&settings.pubkey_path).map_err(|e| {
        eprintln!("{program}: {e}");
        Code::NoKey
    })?;
    Ok(Prepared {
        layout: Layout::new(settings.root),
        key,
    })
}

/// First of all, before anything is created or read: the umask, the working folder and the limits
/// come from whoever ran `pkexec`.
fn reset(program: &str) -> Result<(), Code> {
    reset_process().map_err(|e| {
        eprintln!("{program}: could not prepare to run: {e}");
        Code::Failed
    })
}

/// Prints what an install or rollback did, and picks the exit code.
fn conclude(program: &str, result: Result<Applied, ApplyError>) -> ExitCode {
    match result {
        Ok(applied) => {
            for link in &applied.conflicts {
                eprintln!("{program}: left {link} alone because something else is there");
            }
            for warning in &applied.warnings {
                eprintln!("{program}: {warning}");
            }
            // What the update service reads: the version, then every conflict and warning.
            print!("{}", Report::from(&applied).render());
            finish(Code::Done)
        }
        Err(e) => {
            eprintln!("{program}: {e}");
            finish(e.exit_code())
        }
    }
}

/// `slipway-apply`: install a staged release.
pub fn install_main() -> ExitCode {
    const NAME: &str = "slipway-apply";
    if let Err(code) = reset(NAME) {
        return finish(code);
    }
    let dir = match parse_install(std::env::args().skip(1)) {
        Parsed::Help => {
            println!("{INSTALL_USAGE}");
            return finish(Code::Done);
        }
        Parsed::Bad => {
            eprintln!("{INSTALL_USAGE}");
            return finish(Code::Usage);
        }
        Parsed::Run(dir) => dir,
    };
    let prepared = match prepare(NAME) {
        Ok(prepared) => prepared,
        Err(code) => return finish(code),
    };
    conclude(
        NAME,
        install_with(&prepared.layout, &dir, &prepared.key, &Settling::system()),
    )
}

/// `slipway-rollback`: go back to the previous release. Its polkit action asks for a password.
pub fn rollback_main() -> ExitCode {
    const NAME: &str = "slipway-rollback";
    if let Err(code) = reset(NAME) {
        return finish(code);
    }
    match parse_rollback(std::env::args().skip(1)) {
        Parsed::Help => {
            println!("{ROLLBACK_USAGE}");
            return finish(Code::Done);
        }
        Parsed::Bad => {
            eprintln!("{ROLLBACK_USAGE}");
            return finish(Code::Usage);
        }
        Parsed::Run(()) => {}
    }
    let prepared = match prepare(NAME) {
        Ok(prepared) => prepared,
        Err(code) => return finish(code),
    };
    conclude(
        NAME,
        rollback_with(&prepared.layout, &prepared.key, &Settling::system()),
    )
}

/// `slipway-health`: keep the root-side health record.
pub fn health_main() -> ExitCode {
    const NAME: &str = "slipway-health";
    if let Err(code) = reset(NAME) {
        return finish(code);
    }
    let request = match parse_health(std::env::args().skip(1)) {
        Parsed::Help => {
            println!("{HEALTH_USAGE}");
            return finish(Code::Done);
        }
        Parsed::Bad => {
            eprintln!("{HEALTH_USAGE}");
            return finish(Code::Usage);
        }
        Parsed::Run(request) => request,
    };
    let prepared = match prepare(NAME) {
        Ok(prepared) => prepared,
        Err(code) => return finish(code),
    };
    match request {
        HealthRequest::Confirmed => match note_confirmed(&prepared.layout) {
            Ok(()) => finish(Code::Done),
            Err(e) => {
                eprintln!("{NAME}: {e}");
                finish(e.exit_code())
            }
        },
        HealthRequest::Recover => {
            let boot = std::fs::read_to_string(BOOT_ID_PATH).map(|text| text.trim().to_owned());
            let Some(boot) = boot.ok().filter(|boot| !boot.is_empty()) else {
                eprintln!("{NAME}: cannot tell which boot this is");
                return finish(Code::Failed);
            };
            match note_start(&prepared.layout, &prepared.key, &Settling::system(), &boot) {
                Ok(Started::Fine) => finish(Code::Done),
                Ok(Started::Restored(applied)) => {
                    print!("{}", Report::from(&applied).render());
                    finish(Code::Restored)
                }
                Err(e) => {
                    eprintln!("{NAME}: {e}");
                    finish(e.exit_code())
                }
            }
        }
    }
}
