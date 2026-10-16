//! `slipway-apply <staged-dir>` or `slipway-apply --rollback`: the privileged helper.
//!
//! Started through polkit by the update service. It trusts nothing but the path it is given: the
//! signature and every digest are checked again here against the key installed on this computer.
//! The install root and key come from `/etc/slipway/config.toml`, never from the caller.

use slipway::apply::harden::reset_process;
use slipway::apply::{ApplyError, ExitCode as Code, Selinux, install_with, rollback_with};
use slipway::config::{Config, SYSTEM_CONFIG_PATH};
use slipway::keys::{KeyError, load_key};
use slipway::layout::Layout;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: slipway-apply <staged folder>
       slipway-apply --rollback

Installs the signed release staged in the folder, or goes back to the previous release.

exit codes: 0 done, 1 failed, 2 wrong command, 3 refused (not newer, or nothing to go back to),
            4 the release did not check out, 5 another update is running, 6 no key to check with";

enum Request {
    Install(PathBuf),
    Rollback,
}

fn parse(mut args: impl Iterator<Item = String>) -> Option<Request> {
    let first = args.next()?;
    if args.next().is_some() {
        return None;
    }
    match first.as_str() {
        "--rollback" => Some(Request::Rollback),
        s if s.starts_with('-') => None,
        _ => Some(Request::Install(PathBuf::from(first))),
    }
}

fn finish(code: Code) -> ExitCode {
    ExitCode::from(code as u8)
}

fn main() -> ExitCode {
    // First of all, before anything is created or read: the umask, the working folder and the limits
    // come from whoever ran pkexec.
    let reset = reset_process();
    if let Err(e) = reset {
        eprintln!("slipway-apply: could not prepare to run: {e}");
        return finish(Code::Failed);
    }
    if std::env::args()
        .nth(1)
        .is_some_and(|a| a == "--help" || a == "-h")
    {
        println!("{USAGE}");
        return finish(Code::Done);
    }
    let Some(request) = parse(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return finish(Code::Usage);
    };
    let config = match Config::load(Path::new(SYSTEM_CONFIG_PATH)) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("slipway-apply: {e}");
            return finish(Code::Failed);
        }
    };
    let layout = Layout::new(config.root.clone());
    let relabel = Selinux::system();
    let key = match load_key(&config.pubkey_path) {
        Ok(key) => key,
        Err(KeyError::Missing) => {
            eprintln!("slipway-apply: {}", ApplyError::NoKey);
            return finish(Code::NoKey);
        }
        Err(KeyError::Unreadable) => {
            eprintln!("slipway-apply: the key to check updates with cannot be read");
            return finish(Code::NoKey);
        }
    };
    let result = match request {
        Request::Rollback => rollback_with(&layout, &key, &relabel),
        Request::Install(dir) => install_with(&layout, &dir, &key, &relabel),
    };
    match result {
        Ok(applied) => {
            for link in &applied.conflicts {
                eprintln!("slipway-apply: left {link} alone because something else is there");
            }
            for warning in &applied.warnings {
                eprintln!("slipway-apply: {warning}");
            }
            println!("version {} is now current", applied.version);
            finish(Code::Done)
        }
        Err(e) => {
            eprintln!("slipway-apply: {e}");
            finish(e.exit_code())
        }
    }
}
