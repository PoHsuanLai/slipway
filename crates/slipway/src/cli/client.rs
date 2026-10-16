//! Calling the daemon and putting its answers into one plain line.

use super::wait::{Until, wait};
use super::{Command, Exit};
use crate::config::Config;
use std::time::Duration;
use zbus::proxy::CacheProperties;

const CHECK_LIMIT: Duration = Duration::from_secs(300);
const WORK_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);

/// The daemon's object, as a caller sees it.
#[zbus::proxy(
    interface = "org.quire.Updates1",
    default_service = "org.quire.Updates1",
    default_path = "/org/quire/Updates1"
)]
pub trait UpdatesClient {
    /// Look for a newer version.
    fn check(&self) -> zbus::Result<()>;
    /// Install the newer version.
    fn install(&self) -> zbus::Result<()>;
    /// Go back to the previous version.
    fn rollback(&self) -> zbus::Result<()>;
    /// Where the flow is.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<String>;
    /// The installed version.
    #[zbus(property)]
    fn current(&self) -> zbus::Result<String>;
    /// The version on offer.
    #[zbus(property)]
    fn available(&self) -> zbus::Result<String>;
    /// What went wrong.
    #[zbus(property)]
    fn problem(&self) -> zbus::Result<String>;
    /// When the last check finished.
    #[zbus(property)]
    fn last_checked(&self) -> zbus::Result<i64>;
}

/// What a command answers: the exit code and the one line to print (standard output for `Done` and
/// `NothingToDo`, standard error otherwise).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The exit code.
    pub exit: Exit,
    /// The line.
    pub text: String,
}

impl Reply {
    pub(super) fn done(text: impl Into<String>) -> Self {
        Self {
            exit: Exit::Done,
            text: text.into(),
        }
    }

    pub(super) fn failed(text: impl Into<String>) -> Self {
        Self {
            exit: Exit::Failed,
            text: text.into(),
        }
    }

    fn not_running() -> Self {
        Self {
            exit: Exit::NotRunning,
            text: "The update service is not running.".to_owned(),
        }
    }
}

/// Turns a bus error into a reply: the daemon's own plain refusal, or "not running".
fn from_error(error: &zbus::Error) -> Reply {
    use zbus::fdo::Error as Fdo;
    match error {
        zbus::Error::MethodError(_, Some(why), _) => Reply::failed(why.clone()),
        zbus::Error::FDO(inner) => match &**inner {
            Fdo::Failed(why) => Reply::failed(why.clone()),
            _ => Reply::not_running(),
        },
        _ => Reply::not_running(),
    }
}

async fn proxy() -> zbus::Result<UpdatesClientProxy<'static>> {
    let connection = zbus::Connection::session().await?;
    UpdatesClientProxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

/// Runs `check`, `status`, `install` or `rollback` against the daemon.
pub async fn dispatch(command: Command, config: &Config) -> Reply {
    let Ok(proxy) = proxy().await else {
        return Reply::not_running();
    };
    let result = match command {
        Command::Check => check(&proxy, config).await,
        Command::Status => status(&proxy, config).await,
        Command::Install => install(&proxy, config).await,
        Command::Rollback => rollback(&proxy).await,
        Command::Run | Command::Confirm | Command::Recover | Command::Help => {
            return Reply::failed("not a request for the service");
        }
    };
    result.unwrap_or_else(|e| from_error(&e))
}

async fn problem_reply(proxy: &UpdatesClientProxy<'_>) -> zbus::Result<Reply> {
    let problem = proxy.problem().await?;
    Ok(Reply::failed(if problem.is_empty() {
        "The update could not be completed.".to_owned()
    } else {
        problem
    }))
}

async fn describe_available(
    proxy: &UpdatesClientProxy<'_>,
    config: &Config,
) -> zbus::Result<Reply> {
    let available = proxy.available().await?;
    Ok(Reply::done(if available.is_empty() {
        "Your software is up to date.".to_owned()
    } else {
        format!("{} {available} is available.", config.name)
    }))
}

async fn check(proxy: &UpdatesClientProxy<'_>, config: &Config) -> zbus::Result<Reply> {
    proxy.check().await?;
    match wait(proxy, Until::CheckDone, CHECK_LIMIT, |_| {}).await? {
        Some(state) if state == "failed" => problem_reply(proxy).await,
        Some(_) => describe_available(proxy, config).await,
        None => Ok(Reply::failed("Checking is taking longer than expected.")),
    }
}

async fn status(proxy: &UpdatesClientProxy<'_>, config: &Config) -> zbus::Result<Reply> {
    let current = proxy.current().await?;
    let installed = if current.is_empty() {
        format!("{} is not installed from an update.", config.name)
    } else {
        format!("{} {current} is installed.", config.name)
    };
    if proxy.state().await? == "failed" {
        let problem = proxy.problem().await?;
        return Ok(Reply::done(format!("{installed} {problem}")));
    }
    let newer = describe_available(proxy, config).await?;
    Ok(Reply::done(format!("{installed} {}", newer.text)))
}

async fn install(proxy: &UpdatesClientProxy<'_>, config: &Config) -> zbus::Result<Reply> {
    proxy.install().await?;
    let announce = |state: &str| match state {
        "downloading" => eprintln!("Downloading..."),
        "installing" => eprintln!("Installing..."),
        _ => {}
    };
    match wait(proxy, Until::WorkDone, WORK_LIMIT, announce).await? {
        Some(state) if state == "installed" => {
            let current = proxy.current().await?;
            Ok(Reply::done(format!(
                "{} {current} is installed. Log out and back in to start using it.",
                config.name
            )))
        }
        Some(state) if state == "failed" => problem_reply(proxy).await,
        Some(_) => Ok(Reply {
            exit: Exit::NothingToDo,
            text: "Your software is up to date.".to_owned(),
        }),
        None => Ok(Reply::failed("Installing is taking longer than expected.")),
    }
}

async fn rollback(proxy: &UpdatesClientProxy<'_>) -> zbus::Result<Reply> {
    proxy.rollback().await?;
    match wait(proxy, Until::WorkDone, WORK_LIMIT, |_| {}).await? {
        Some(state) if state == "installed" => Ok(Reply::done(
            "The previous version is back. Log out and back in to use it.",
        )),
        Some(_) => problem_reply(proxy).await,
        None => Ok(Reply::failed("Going back is taking longer than expected.")),
    }
}
