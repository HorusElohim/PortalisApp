//! Step 2 — the device log: enrol, revoke, replay, and six attacks refused.
//!
//! A person is a signed append-only log of their devices, not an account row
//! (D2). The reason is one attack: if the service holds the list, it can add
//! a device, and an owner about to seal a content key will seal to it. A log
//! makes that impossible rather than auditable — extending it needs a key
//! already inside it.
//!
//! So most of the output is refusals, each with its own reason, because
//! "invalid log" would let two different bugs look alike.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 02-device-log`.

use portalis_nexus_demo::{NOW, Person, section, short};
use portalis_nexus_protocol::{Action, DeviceLog, DeviceLogError, LogEntry};

fn main() {
    let laptop = Person::new("laptop", 1);
    let phone = Person::new("phone", 2);
    let tablet = Person::new("tablet", 3);
    let impostor = Person::new("an impostor", 9);

    section("A log, built by its own devices");
    let root = laptop.root_entry();
    let add_phone = laptop.states(2, Some(&root), Action::Enrol, &phone);
    let add_tablet = laptop.states(3, Some(&add_phone), Action::Enrol, &tablet);
    let entries = vec![root, add_phone, add_tablet];
    report(&DeviceLog::replay(&entries).expect("the owner's own devices"));
    println!("  Every entry after the first is signed by a device already in");
    println!("  the log. That is the whole rule.");

    section("Revoking the tablet");
    // Signed by the phone: any enrolled device may revoke another, because
    // authority belongs to the log rather than to one device.
    let revoke = laptop.states_by(4, Some(&add_tablet), Action::Revoke, &tablet, &phone);
    let revoked_entries = [entries.clone(), vec![revoke]].concat();
    let revoked = DeviceLog::replay(&revoked_entries).expect("a revocation");
    report(&revoked);
    println!("  The tablet stays in the history: rotating a key needs to know");
    println!("  what could once read, not only what can now.");

    section("Six attacks");
    let attacks: [(&str, Vec<LogEntry>, &str); 5] = [
        (
            "a service invents a device and signs for it",
            [
                entries.clone(),
                vec![laptop.states_by(4, Some(&add_tablet), Action::Enrol, &impostor, &impostor)],
            ]
            .concat(),
            "the key would be sealed to a device the owner never had",
        ),
        (
            "a revoked device enrols a new one",
            [
                revoked_entries.clone(),
                vec![laptop.states_by(
                    5,
                    revoked_entries.last(),
                    Action::Enrol,
                    &impostor,
                    &tablet,
                )],
            ]
            .concat(),
            "a stolen laptop could re-admit itself under a new key",
        ),
        (
            "an entry claims one author but is signed by another",
            [
                entries.clone(),
                vec![impostor.resign(laptop.states(
                    4,
                    Some(&add_tablet),
                    Action::Enrol,
                    &impostor,
                ))],
            ]
            .concat(),
            "claiming authorship is not the same as holding the key",
        ),
        (
            "the entries are reordered",
            reordered(&entries),
            "order is what decides who was authorized when",
        ),
        (
            "a second root is appended",
            [entries.clone(), vec![phone.root_entry()]].concat(),
            "two roots would mean two people, or one overwritten",
        ),
    ];
    for (what, log, cost) in attacks {
        refused(what, DeviceLog::replay(&log), cost);
    }

    // Nothing forged at all: an older genuine log, served to undo a
    // revocation. This is why the highest verified state is held.
    refused(
        "an older log is served in place of the current one",
        revoked.adopt(&entries),
        "the tablet's revocation would be undone by an entirely genuine log",
    );

    section("A fork is not an update");
    let rival = vec![
        laptop.root_entry(),
        add_phone,
        laptop.states(3, Some(&add_phone), Action::Enrol, &impostor),
    ];
    assert!(DeviceLog::replay(&rival).is_ok(), "the fork is valid alone");
    println!("  The forked log verifies on its own — that is what makes it");
    println!("  dangerous, and why it is caught against what is already held.");
    refused(
        "a log that disagrees about what already happened",
        DeviceLog::replay(&entries).expect("held").adopt(&rival),
        "a service splitting two contacts' view of the same person",
    );

    println!("\nSix attacks, six distinct reasons. No log was extended without");
    println!("a key that was already inside it.");
}

fn reordered(entries: &[LogEntry]) -> Vec<LogEntry> {
    let mut swapped = entries.to_vec();
    swapped.swap(1, 2);
    swapped
}

fn report(log: &DeviceLog) {
    println!(
        "  sequence {} · hash {}",
        log.sequence(),
        short(&log.hash())
    );
    for device in log.history() {
        let state = device.revoked_at_unix_ns.map_or_else(
            || "authorized".to_owned(),
            |at| format!("revoked at {}", at - NOW),
        );
        println!("    {} — {state}", short(&device.signing_key));
    }
}

fn refused(what: &str, outcome: Result<DeviceLog, DeviceLogError>, cost: &str) {
    let error = outcome.expect_err("this attack must be refused");
    println!("\n  {what}");
    println!("    refused: {error}");
    println!("    without it: {cost}");
}
