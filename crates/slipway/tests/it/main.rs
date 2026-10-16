//! Effectful tests: every one runs in a temporary directory standing in for the install root and for
//! `/`; nothing touches the real system or the session bus.

mod apply_hardening;
mod apply_hostile;
mod apply_install;
mod apply_rollback;
mod download_resume;
mod elevate_run;
mod exports_applied;
mod feed_stage;
mod health_start;
mod small;
mod support;
mod system_applied;
