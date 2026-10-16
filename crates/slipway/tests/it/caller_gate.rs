//! Who may ask the update service for what: the roles each call answers, on a private bus with the
//! sender's standing introduced (the test seam of the caller identification), and the refusals as a
//! caller sees them.

use crate::feed_stage::{block_on, engine_serving};
use crate::private_bus::{PrivateBus, name_of};
use crate::support::Sandbox;
use peephole_bus::peephole::fake::{introduced_program, introduced_service, introduced_unverified};
use peephole_bus::peephole_core::{Pin, Proof, Standing, Table, WhyNot};
use slipway::problem::Problem;
use slipway::service::{BUS_NAME, Gate, PATH, Updates, peers_on};
use slipway::state::StateStore;

fn service(role: &str, proof: Proof) -> Standing {
    introduced_service("org.quire.Test", role, proof, Pin::AtConnect).expect("a standing")
}

/// (case, who the caller is, may install, may roll back, may check)
type Case = (&'static str, Standing, bool, bool, bool);

fn cases() -> Vec<Case> {
    vec![
        (
            "the shell, proved by its unit",
            service("shell", Proof::Full),
            true,
            true,
            true,
        ),
        (
            "the shell, image only",
            service("shell", Proof::Exe),
            false,
            false,
            true,
        ),
        (
            "Settings",
            service("settings", Proof::Exe),
            true,
            true,
            true,
        ),
        (
            "Settings, proved by its unit",
            service("settings", Proof::Full),
            true,
            true,
            true,
        ),
        (
            "the slipway program",
            service("updater_cli", Proof::Exe),
            true,
            false,
            true,
        ),
        (
            "the update service itself",
            service("updater", Proof::Full),
            false,
            false,
            true,
        ),
        (
            "the portal",
            service("portal", Proof::Full),
            false,
            false,
            true,
        ),
        (
            "the assistant",
            service("agent_tier", Proof::Full),
            false,
            false,
            true,
        ),
        (
            "a root-owned program in no row",
            introduced_program("/usr/bin/evil").expect("a standing"),
            false,
            false,
            true,
        ),
        (
            "a program the person can rewrite",
            introduced_unverified(WhyNot::ImageWritable),
            false,
            false,
            true,
        ),
    ]
}

fn expect(allowed: bool, answer: Result<(), Problem>, refusal: Problem, case: &str, call: &str) {
    let want = if allowed { Ok(()) } else { Err(refusal) };
    assert_eq!(answer, want, "{case}: {call}");
}

#[test]
fn each_call_answers_only_the_roles_it_names() {
    block_on(async {
        let bus = PrivateBus::start();
        let (server, client) = (bus.connect().await, bus.connect().await);
        for (case, standing, install, rollback, check) in cases() {
            let peers = peers_on(server.clone(), Table::empty());
            peers.introduce(&name_of(&client), standing);
            let gate = Gate::new(peers);
            let sender = name_of(&client);
            let sender = Some(sender.as_str());
            expect(
                install,
                gate.install(sender).await,
                Problem::InstallNotFromHere,
                case,
                "install",
            );
            expect(
                rollback,
                gate.rollback(sender).await,
                Problem::RollbackNotFromHere,
                case,
                "rollback",
            );
            expect(
                check,
                gate.check(sender).await,
                Problem::CallerUnknown,
                case,
                "check",
            );
        }
    });
}

#[test]
fn a_call_with_no_sender_or_from_nobody_on_the_bus_is_refused() {
    block_on(async {
        let bus = PrivateBus::start();
        let gate = Gate::new(peers_on(bus.connect().await, Table::empty()));
        for sender in [None, Some(":1.9999"), Some("not a bus name")] {
            expect(
                false,
                gate.install(sender).await,
                Problem::InstallNotFromHere,
                "unknown",
                "install",
            );
            expect(
                false,
                gate.rollback(sender).await,
                Problem::RollbackNotFromHere,
                "unknown",
                "rollback",
            );
            expect(
                false,
                gate.check(sender).await,
                Problem::CallerUnknown,
                "unknown",
                "check",
            );
        }
    });
}

/// The error a caller of the served object sees: its name and its sentence.
fn seen(error: &zbus::Error) -> (String, Option<String>) {
    use zbus::DBusError;
    match error {
        zbus::Error::MethodError(name, why, _) => (name.to_string(), why.clone()),
        zbus::Error::FDO(inner) => (
            (**inner).name().to_string(),
            (**inner).description().map(str::to_owned),
        ),
        other => (other.to_string(), None),
    }
}

#[test]
fn a_refused_caller_is_told_access_is_denied_in_plain_words() {
    // (who the caller is, method, argument is a version, the name of the error, the sentence)
    let denied_install = Problem::InstallNotFromHere.plain();
    let denied_rollback = Problem::RollbackNotFromHere.plain();
    let table: Vec<(Standing, &str, &str, Option<&str>)> = vec![
        (
            service("portal", Proof::Full),
            "Install",
            "org.quire.Updates1.Error.AccessDenied",
            Some(denied_install),
        ),
        (
            service("updater_cli", Proof::Exe),
            "Rollback",
            "org.freedesktop.DBus.Error.AccessDenied",
            Some(denied_rollback),
        ),
        (
            service("shell", Proof::Exe),
            "Rollback",
            "org.freedesktop.DBus.Error.AccessDenied",
            Some(denied_rollback),
        ),
        // Let through to the call itself, which refuses text that is not a version.
        (
            service("updater_cli", Proof::Exe),
            "Install",
            "org.quire.Updates1.Error.NotAVersion",
            None,
        ),
        (
            service("shell", Proof::Full),
            "Install",
            "org.quire.Updates1.Error.NotAVersion",
            None,
        ),
    ];
    for (standing, method, name, sentence) in table {
        let answer = block_on(async {
            let bus = PrivateBus::start();
            let (server, client) = (bus.connect().await, bus.connect().await);
            let sandbox = Sandbox::new();
            let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
            let peers = peers_on(server.clone(), Table::empty());
            peers.introduce(&name_of(&client), standing);
            let updates = Updates::new(engine_serving(&sandbox, "0.2.0", &store), Gate::new(peers));
            server
                .object_server()
                .at(PATH, updates)
                .await
                .expect("serve");
            server.request_name(BUS_NAME).await.expect("the name");
            let reply = match method {
                "Install" => {
                    client
                        .call_method(
                            Some(BUS_NAME),
                            PATH,
                            Some("org.quire.Updates1"),
                            method,
                            &("nope",),
                        )
                        .await
                }
                _ => {
                    client
                        .call_method(
                            Some(BUS_NAME),
                            PATH,
                            Some("org.quire.Updates1"),
                            method,
                            &(),
                        )
                        .await
                }
            };
            reply.expect_err("refused")
        });
        let (got, said) = seen(&answer);
        assert_eq!(got, name, "{method}");
        if let Some(sentence) = sentence {
            assert_eq!(said.as_deref(), Some(sentence), "{method}");
        }
    }
}
