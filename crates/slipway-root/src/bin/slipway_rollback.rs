//! `slipway-rollback`: the privileged helper that goes back to the previous release.
//!
//! Started through its own polkit action, which asks for the administrator's password: going back is a
//! downgrade, and a program of the person must not be able to ask for one. The work is in
//! `slipway_root::helper`.

fn main() -> std::process::ExitCode {
    slipway_root::helper::rollback_main()
}
