//! `slipway`: the update service (`run`) and its command line.

use slipway::bootstrap::production;
use slipway::cli::{Command, Exit, Reply, USAGE, dispatch, parse, recover, vouch};
use slipway::config::{Config, SYSTEM_CONFIG_PATH};
use slipway::dirs::Dirs;
use slipway::elevate::{Helpers, Pkexec};
use slipway::service::run;
use slipway::state::StateStore;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let command = match parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(usage) => {
            eprintln!("{usage}");
            return exit(Exit::Usage);
        }
    };
    if command == Command::Help {
        println!("{USAGE}");
        return exit(Exit::Done);
    }
    if command == Command::Recover {
        return recover_session();
    }
    let Some(dirs) = Dirs::resolve(|name| std::env::var_os(name)) else {
        eprintln!("slipway: cannot tell where your settings are (no home folder)");
        return exit(Exit::Failed);
    };
    let config = match Config::load_layered(Path::new(SYSTEM_CONFIG_PATH), &dirs.config_file) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("slipway: {e}");
            return exit(Exit::Failed);
        }
    };
    match command {
        Command::Run => run_service(config, &dirs),
        Command::Confirm => confirm_session(&config, &dirs),
        Command::Recover => recover_session(),
        _ => ask_service(command, &config),
    }
}

fn exit(code: Exit) -> ExitCode {
    ExitCode::from(code as u8)
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

fn run_service(config: Config, dirs: &Dirs) -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let outcome = runtime().map_err(|e| e.to_string()).and_then(|rt| {
        rt.block_on(run(production(config, dirs)))
            .map_err(|e| e.to_string())
    });
    match outcome {
        Ok(()) => exit(Exit::Done),
        Err(why) => {
            eprintln!("slipway: {why}");
            exit(Exit::Failed)
        }
    }
}

fn say(reply: &Reply) -> ExitCode {
    match reply.exit {
        Exit::Done | Exit::NothingToDo | Exit::Restored => println!("{}", reply.text),
        _ => eprintln!("{}", reply.text),
    }
    exit(reply.exit)
}

/// The session says the shell is up. Needs no service and no bus: the person's own state, then the
/// system's health record.
fn confirm_session(config: &Config, dirs: &Dirs) -> ExitCode {
    let elevate = Pkexec::new(Helpers::from(config));
    say(&vouch(
        &config.root,
        &StateStore::new(dirs.state_file.clone()),
        &elevate,
    ))
}

/// The session is about to start: go back to the previous version if this one keeps failing to start.
/// Reads only the system settings, so it runs even where there is no home folder yet.
fn recover_session() -> ExitCode {
    match Config::load(Path::new(SYSTEM_CONFIG_PATH)) {
        Ok(config) => say(&recover(&Pkexec::new(Helpers::from(&config)))),
        Err(e) => {
            eprintln!("slipway: {e}");
            exit(Exit::Failed)
        }
    }
}

fn ask_service(command: Command, config: &Config) -> ExitCode {
    let Ok(rt) = runtime() else {
        return exit(Exit::Failed);
    };
    say(&rt.block_on(dispatch(command, config)))
}
