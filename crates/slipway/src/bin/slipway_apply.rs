//! `slipway-apply <staged-dir>`: the privileged helper that installs a release.
//!
//! Started through polkit by the update service. It trusts nothing but the path it is given: the
//! signature and every digest are checked again here against the key installed on this computer.
//! The install root and key come from `/etc/slipway/config.toml`, never from the caller. It cannot go
//! back to an earlier release (`slipway-rollback` does, behind a password). The work is in
//! `slipway::helper`.

fn main() -> std::process::ExitCode {
    slipway::helper::install_main()
}
