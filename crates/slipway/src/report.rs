//! What the privileged helper tells the update service when it succeeds. The type and its text
//! format live in `slipway-root` (the helper prints it); the service logs it here.

pub use slipway_root::report::Report;

/// Writes each conflict and warning to the log.
pub fn log(report: &Report) {
    for path in &report.conflicts {
        tracing::warn!("the update left {path} alone because something else is there");
    }
    for warning in &report.warnings {
        tracing::warn!("the update finished with a problem: {warning}");
    }
}
