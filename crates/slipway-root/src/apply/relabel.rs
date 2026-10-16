//! Gives files placed by the helper their SELinux labels, on computers that use SELinux.
//!
//! A tree unpacked under `/opt/quire` gets the generic label of its parent, which an enforcing policy
//! may refuse to run (a system service starting a program from there). `restorecon` sets each file's
//! label from the policy. Where SELinux is not in use, or the tool is not installed, nothing is done and
//! that is not a problem: labels are best effort and never fail an install.

use super::settle::Change;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Whether the SELinux status file the kernel exposes is there: the sign that SELinux is in use.
pub const SELINUX_ENFORCE_FILE: &str = "/sys/fs/selinux/enforce";
/// Where the label tool is looked for. A fixed list, so a root helper never depends on `PATH`.
pub const RESTORECON_PATHS: [&str; 3] = [
    "/usr/sbin/restorecon",
    "/usr/bin/restorecon",
    "/sbin/restorecon",
];

/// What a relabel came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relabeled {
    /// SELinux is not in use, or has no tool here: nothing to do.
    NotNeeded,
    /// The labels were set from the policy.
    Done,
    /// The tool ran and failed; the files are installed, only their labels may be generic.
    Failed(String),
}

/// How far a relabel reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Everything below the given folders.
    Tree,
    /// Just the given paths.
    Paths,
}

/// What the helper asks of the computer after it placed files: their labels, and (for system files)
/// telling the system about them.
pub trait Relabel {
    /// Sets the labels of `paths` (and, for [`Reach::Tree`], everything below them).
    fn relabel(&self, paths: &[PathBuf], reach: Reach) -> Relabeled;

    /// Tells the system about system files just placed or removed; returns a plain note for each thing
    /// that did not work (never an error: the files are in place either way). Nothing by default.
    fn settle(&self, _changes: &[Change]) -> Vec<String> {
        Vec::new()
    }
}

/// Does nothing: for tests, and for a helper used on a computer without SELinux.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRelabel;

impl Relabel for NoRelabel {
    fn relabel(&self, _: &[PathBuf], _: Reach) -> Relabeled {
        Relabeled::NotNeeded
    }
}

/// What running a program came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran {
    /// The program is not at that path.
    Missing,
    /// It ran and succeeded.
    Succeeded,
    /// It ran and failed, or could not be started; the text says why.
    Failed(String),
}

/// Runs a program with arguments. The seam that lets tests stand in for `restorecon`.
pub trait RunProgram {
    /// Runs `program` with `args` and waits for it.
    fn run(&self, program: &Path, args: &[OsString]) -> Ran;
}

/// Runs real programs, with no input and no output kept.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemRunner;

impl RunProgram for SystemRunner {
    fn run(&self, program: &Path, args: &[OsString]) -> Ran {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        match output {
            Ok(out) if out.status.success() => Ran::Succeeded,
            Ok(out) => Ran::Failed(
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .next()
                    .unwrap_or("it failed")
                    .to_owned(),
            ),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ran::Missing,
            Err(e) => Ran::Failed(e.to_string()),
        }
    }
}

/// Relabels with `restorecon -F` (and `-R` for a tree) when SELinux is in use.
#[derive(Debug)]
pub struct Selinux<R> {
    status_file: PathBuf,
    tools: Vec<PathBuf>,
    runner: R,
}

impl Selinux<SystemRunner> {
    /// The real system: the kernel's status file and the usual places of the tool.
    pub fn system() -> Self {
        Self::new(
            PathBuf::from(SELINUX_ENFORCE_FILE),
            RESTORECON_PATHS.map(PathBuf::from).to_vec(),
            SystemRunner,
        )
    }
}

impl<R: RunProgram> Selinux<R> {
    /// Relabels when `status_file` exists, with the first of `tools` that is installed.
    pub fn new(status_file: PathBuf, tools: Vec<PathBuf>, runner: R) -> Self {
        Self {
            status_file,
            tools,
            runner,
        }
    }
}

impl<R: RunProgram> Relabel for Selinux<R> {
    fn relabel(&self, paths: &[PathBuf], reach: Reach) -> Relabeled {
        if paths.is_empty() || !self.status_file.exists() {
            return Relabeled::NotNeeded;
        }
        let mut args: Vec<OsString> = vec![match reach {
            Reach::Tree => "-RF".into(),
            Reach::Paths => "-F".into(),
        }];
        args.extend(paths.iter().map(|p| p.as_os_str().to_owned()));
        for tool in &self.tools {
            match self.runner.run(tool, &args) {
                Ran::Missing => {}
                Ran::Succeeded => return Relabeled::Done,
                Ran::Failed(why) => return Relabeled::Failed(why),
            }
        }
        Relabeled::NotNeeded
    }
}
