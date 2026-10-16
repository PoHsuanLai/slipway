//! What the privileged helper tells the update service when it succeeds: which release is now current,
//! which links and system files it left alone because something else was in the way, and what did not
//! work but did not stop the update.
//!
//! The helper prints it on its standard output, one line each (`current <version>`, `left-alone <path>`,
//! `warning <text>`), and the service parses it. A helper that succeeds with something to say is not
//! the same as one that has nothing to say: a person who edited `/etc/quire/intentd-callers.toml` must
//! be told that the new release's table was not put in place.

use semver::Version;

const CURRENT: &str = "current ";
const LEFT_ALONE: &str = "left-alone ";
const WARNING: &str = "warning ";

/// What an install or rollback reported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The release now current, when the helper said.
    pub version: Option<Version>,
    /// Links and system files left alone because something else is there.
    pub conflicts: Vec<String>,
    /// Things that did not work but did not stop the update.
    pub warnings: Vec<String>,
}

/// One line of text: no line breaks, so a message cannot add lines of its own.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

impl Report {
    /// The text the helper prints.
    pub fn render(&self) -> String {
        let mut text = String::new();
        if let Some(version) = &self.version {
            text += &format!("{CURRENT}{version}\n");
        }
        for path in &self.conflicts {
            text += &format!("{LEFT_ALONE}{}\n", one_line(path));
        }
        for warning in &self.warnings {
            text += &format!("{WARNING}{}\n", one_line(warning));
        }
        text
    }

    /// Reads what [`Report::render`] wrote. Lines it does not know are ignored.
    pub fn parse(text: &str) -> Self {
        let mut report = Self::default();
        for line in text.lines() {
            if let Some(version) = line.strip_prefix(CURRENT) {
                report.version = Version::parse(version.trim()).ok();
            } else if let Some(path) = line.strip_prefix(LEFT_ALONE) {
                report.conflicts.push(path.to_owned());
            } else if let Some(warning) = line.strip_prefix(WARNING) {
                report.warnings.push(warning.to_owned());
            }
        }
        report
    }

    /// Writes each conflict and warning to the log.
    pub fn log(&self) {
        for path in &self.conflicts {
            tracing::warn!("the update left {path} alone because something else is there");
        }
        for warning in &self.warnings {
            tracing::warn!("the update finished with a problem: {warning}");
        }
    }
}

impl From<&crate::apply::Applied> for Report {
    fn from(applied: &crate::apply::Applied) -> Self {
        Self {
            version: Some(applied.version.clone()),
            conflicts: applied.conflicts.clone(),
            warnings: applied.warnings.clone(),
        }
    }
}
