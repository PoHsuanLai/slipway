//! Effectful tests: every one runs in a temporary directory standing in for the install root and for
//! `/`; nothing touches the real system or the session bus.

mod download_resume;
mod elevate_run;
mod feed_stage;
mod health_start;
mod privileged_programs;
mod session_guard;
mod small;
mod support;
