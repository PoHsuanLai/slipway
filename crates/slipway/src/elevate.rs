//! Running the privileged helpers through `pkexec`: one installs a staged release, one goes back to the
//! previous release (and asks for the administrator's password), one keeps the root-side health record.

use crate::config::Config;
use crate::report::Report;
use slipway_root::apply::ExitCode;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a helper run may take before the service stops waiting: an install of a large bundle takes
/// minutes, and a password prompt waits for a person.
pub const HELPER_PATIENCE: Duration = Duration::from_secs(30 * 60);
/// How often the service looks whether the helper has finished.
const POLL: Duration = Duration::from_millis(100);
/// The most of the helper's output kept; the rest is read and dropped so it never blocks on a full pipe.
const MAX_OUTPUT_BYTES: u64 = 1 << 20;

/// A helper run that did not succeed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the update helper failed ({code:?}): {detail}")]
pub struct ApplyFailure {
    /// Its exit code, `None` when it could not be started, was killed or ran out of time.
    pub code: Option<i32>,
    /// The last line it printed on its error output.
    pub detail: String,
}

/// What the health helper did when asked to guard a start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Nothing to do: the release is vouched for, still has boots left, or there is nothing to go back to.
    Fine,
    /// The previous release was put back; the session should start again.
    Restored(Report),
}

/// Something that can install a staged release, go back, or keep the health record, with privilege.
pub trait Elevate: Send + Sync {
    /// Installs the release staged in `staged`.
    fn install(&self, staged: &Path) -> Result<Report, ApplyFailure>;
    /// Goes back to the previous release because the person asked to (the system asks for the
    /// administrator's password).
    fn rollback(&self) -> Result<Report, ApplyFailure>;
    /// Notes a start of the current release and goes back to the previous one if it has failed to start
    /// in too many boots (no password: the helper decides from what only it can see).
    fn recover(&self) -> Result<Recovery, ApplyFailure>;
    /// Records that the session started well on the current release.
    fn confirmed(&self) -> Result<(), ApplyFailure>;
}

/// Where the three privileged programs are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Helpers {
    /// Installs a staged release.
    pub install: PathBuf,
    /// Goes back to the previous release.
    pub rollback: PathBuf,
    /// Keeps the root-side health record.
    pub health: PathBuf,
}

impl From<&Config> for Helpers {
    fn from(config: &Config) -> Self {
        Self {
            install: config.apply_path.clone(),
            rollback: config.rollback_path.clone(),
            health: config.health_path.clone(),
        }
    }
}

/// Asks polkit to run the helpers as root.
#[derive(Clone, Debug)]
pub struct Pkexec {
    program: PathBuf,
    helpers: Helpers,
    patience: Duration,
}

/// What a helper run left, once it finished.
struct Finished {
    status: ExitStatus,
    out: String,
    err: String,
}

fn failure(why: impl ToString) -> ApplyFailure {
    ApplyFailure {
        code: None,
        detail: why.to_string(),
    }
}

/// Reads a pipe on a thread: the first `MAX_OUTPUT_BYTES`, then the rest thrown away.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = (&mut pipe).take(MAX_OUTPUT_BYTES).read_to_end(&mut kept);
            let _ = io::copy(&mut pipe, &mut io::sink());
        }
        String::from_utf8_lossy(&kept).into_owned()
    })
}

/// Waits for `child` at most `patience`; `None` when it did not finish.
fn wait_within(child: &mut Child, patience: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + patience;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(POLL);
    }
}

impl Pkexec {
    /// Runs `helpers` through `pkexec`.
    pub fn new(helpers: Helpers) -> Self {
        Self::with_program(PathBuf::from("pkexec"), helpers, HELPER_PATIENCE)
    }

    /// Runs `helpers` through `program` (`pkexec`; a test passes a shell) and gives up after `patience`.
    pub fn with_program(program: PathBuf, helpers: Helpers, patience: Duration) -> Self {
        Self {
            program,
            helpers,
            patience,
        }
    }

    fn start(&self, helper: &Path, args: &[&std::ffi::OsStr]) -> Result<Finished, ApplyFailure> {
        let mut child = Command::new(&self.program)
            .arg(helper)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(failure)?;
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        let Some(status) = wait_within(&mut child, self.patience).map_err(failure)? else {
            // The helper may be a root process we cannot signal; stop waiting either way.
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Err(failure("the update helper did not finish in time"));
        };
        Ok(Finished {
            status,
            out: out.join().unwrap_or_default(),
            err: err.join().unwrap_or_default(),
        })
    }

    fn run(&self, helper: &Path, args: &[&OsStr]) -> Result<Report, ApplyFailure> {
        let finished = self.start(helper, args)?;
        if finished.status.success() {
            return Ok(Report::parse(&finished.out));
        }
        Err(refused(&finished))
    }
}

fn refused(finished: &Finished) -> ApplyFailure {
    let detail = finished
        .err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .to_owned();
    ApplyFailure {
        code: finished.status.code(),
        detail,
    }
}

impl Elevate for Pkexec {
    fn install(&self, staged: &Path) -> Result<Report, ApplyFailure> {
        self.run(&self.helpers.install, &[staged.as_os_str()])
    }

    fn rollback(&self) -> Result<Report, ApplyFailure> {
        self.run(&self.helpers.rollback, &[])
    }

    fn recover(&self) -> Result<Recovery, ApplyFailure> {
        let finished = self.start(&self.helpers.health, &[OsStr::new("recover")])?;
        match finished.status.code() {
            Some(0) => Ok(Recovery::Fine),
            Some(code) if code == ExitCode::Restored as i32 => {
                Ok(Recovery::Restored(Report::parse(&finished.out)))
            }
            _ => Err(refused(&finished)),
        }
    }

    fn confirmed(&self) -> Result<(), ApplyFailure> {
        let finished = self.start(&self.helpers.health, &[OsStr::new("confirmed")])?;
        if finished.status.success() {
            Ok(())
        } else {
            Err(refused(&finished))
        }
    }
}
