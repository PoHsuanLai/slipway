//! Running the privileged helper: `pkexec <helper> <staged dir>` or `pkexec <helper> --rollback`.

use crate::report::Report;
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

/// Something that can install a staged release or roll back with privilege.
pub trait Elevate: Send + Sync {
    /// Installs the release staged in `staged`.
    fn install(&self, staged: &Path) -> Result<Report, ApplyFailure>;
    /// Goes back to the previous release.
    fn rollback(&self) -> Result<Report, ApplyFailure>;
}

/// Asks polkit to run the helper as root.
#[derive(Clone, Debug)]
pub struct Pkexec {
    program: PathBuf,
    helper: PathBuf,
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
    /// Runs the helper installed at `helper` through `pkexec`.
    pub fn new(helper: PathBuf) -> Self {
        Self::with_program(PathBuf::from("pkexec"), helper, HELPER_PATIENCE)
    }

    /// Runs `helper` through `program` (`pkexec`; a test passes a shell) and gives up after `patience`.
    pub fn with_program(program: PathBuf, helper: PathBuf, patience: Duration) -> Self {
        Self {
            program,
            helper,
            patience,
        }
    }

    fn start(&self, args: &[&std::ffi::OsStr]) -> Result<Finished, ApplyFailure> {
        let mut child = Command::new(&self.program)
            .arg(&self.helper)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(failure)?;
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        match wait_within(&mut child, self.patience).map_err(failure)? {
            Some(status) => Ok(Finished {
                status,
                out: out.join().unwrap_or_default(),
                err: err.join().unwrap_or_default(),
            }),
            None => {
                // The helper may be a root process we cannot signal; stop waiting either way.
                let _ = child.kill();
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                Err(failure("the update helper did not finish in time"))
            }
        }
    }

    fn run(&self, args: &[&std::ffi::OsStr]) -> Result<Report, ApplyFailure> {
        let finished = self.start(args)?;
        if finished.status.success() {
            return Ok(Report::parse(&finished.out));
        }
        let detail = finished
            .err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default()
            .to_owned();
        Err(ApplyFailure {
            code: finished.status.code(),
            detail,
        })
    }
}

impl Elevate for Pkexec {
    fn install(&self, staged: &Path) -> Result<Report, ApplyFailure> {
        self.run(&[staged.as_os_str()])
    }

    fn rollback(&self) -> Result<Report, ApplyFailure> {
        self.run(&[std::ffi::OsStr::new("--rollback")])
    }
}
