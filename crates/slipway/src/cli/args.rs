//! Reading the command line.

/// The help text.
pub const USAGE: &str = "usage: slipway <command>

  run        keep the update service going (the session starts this)
  check      look for a newer version now
  status     say what is installed and whether a newer version is waiting
  install    install the newer version
  rollback   go back to the version before
  confirm    tell the service this session started well (the session runs this)

exit codes: 0 done, 1 failed, 2 wrong command, 3 the service is not running,
            4 nothing newer to install";

/// What was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Run the service.
    Run,
    /// Check now.
    Check,
    /// Report.
    Status,
    /// Install.
    Install,
    /// Roll back.
    Rollback,
    /// Confirm this session is healthy.
    Confirm,
    /// Print the help text.
    Help,
}

/// The command line was not understood.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("slipway: {0}\n{USAGE}")]
pub struct UsageError(pub String);

/// Reads the arguments after the program name.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, UsageError> {
    let mut args = args.into_iter();
    let first = args
        .next()
        .ok_or_else(|| UsageError("no command given".to_owned()))?;
    if let Some(extra) = args.next() {
        return Err(UsageError(format!("unexpected {extra:?}")));
    }
    match first.as_str() {
        "run" => Ok(Command::Run),
        "check" => Ok(Command::Check),
        "status" => Ok(Command::Status),
        "install" => Ok(Command::Install),
        "rollback" => Ok(Command::Rollback),
        "confirm" => Ok(Command::Confirm),
        "help" | "--help" | "-h" => Ok(Command::Help),
        other => Err(UsageError(format!("unknown command {other:?}"))),
    }
}
