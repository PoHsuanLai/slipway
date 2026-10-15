//! Running the privileged helper: `pkexec <helper> <staged dir>` or `pkexec <helper> --rollback`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A helper run that did not succeed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the update helper failed ({code:?}): {detail}")]
pub struct ApplyFailure {
    /// Its exit code, `None` when it could not be started or was killed.
    pub code: Option<i32>,
    /// The last line it printed on its error output.
    pub detail: String,
}

/// Something that can install a staged release or roll back with privilege.
pub trait Elevate: Send + Sync {
    /// Installs the release staged in `staged`.
    fn install(&self, staged: &Path) -> Result<(), ApplyFailure>;
    /// Goes back to the previous release.
    fn rollback(&self) -> Result<(), ApplyFailure>;
}

/// Asks polkit to run the helper as root.
#[derive(Clone, Debug)]
pub struct Pkexec {
    helper: PathBuf,
}

impl Pkexec {
    /// Runs the helper installed at `helper`.
    pub fn new(helper: PathBuf) -> Self {
        Self { helper }
    }

    fn run(&self, args: &[&std::ffi::OsStr]) -> Result<(), ApplyFailure> {
        let output = Command::new("pkexec")
            .arg(&self.helper)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| ApplyFailure {
                code: None,
                detail: e.to_string(),
            })?;
        if output.status.success() {
            return Ok(());
        }
        let detail = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default()
            .to_owned();
        Err(ApplyFailure {
            code: output.status.code(),
            detail,
        })
    }
}

impl Elevate for Pkexec {
    fn install(&self, staged: &Path) -> Result<(), ApplyFailure> {
        self.run(&[staged.as_os_str()])
    }

    fn rollback(&self) -> Result<(), ApplyFailure> {
        self.run(&[std::ffi::OsStr::new("--rollback")])
    }
}
