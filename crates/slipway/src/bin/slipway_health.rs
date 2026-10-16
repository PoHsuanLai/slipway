//! `slipway-health recover` or `slipway-health confirmed`: the privileged half of the health check.
//!
//! Started through polkit, with no password, by the session. It keeps a record only root can write,
//! counts only what only root can see (the boot), and puts the previous release back when the current
//! one has failed to start in too many boots. The work is in `slipway::helper`.

fn main() -> std::process::ExitCode {
    slipway::helper::health_main()
}
