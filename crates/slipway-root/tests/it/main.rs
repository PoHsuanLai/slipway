//! Effectful tests of the privileged side: every one runs in a temporary directory standing in for the
//! install root and for `/`; nothing touches the real system.

mod apply_hardening;
mod apply_hostile;
mod apply_install;
mod apply_rollback;
mod exports_applied;
mod programs;
mod small;
mod support;
mod system_applied;
