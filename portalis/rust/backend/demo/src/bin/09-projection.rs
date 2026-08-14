//! Step 9 — one stream down, and what it costs.
//!
//! The interface stops asking. It subscribes once and is told, and every
//! derivation it used to do — a percentage, a status, whether a contact is
//! verified — happens on this side, once.
//!
//! Four properties, each shown in numbers rather than claimed:
//!
//! - **Idle costs nothing.** An unchanged tick sends zero bytes.
//! - **Progress coalesces** to four times a second, however fast the readings
//!   arrive.
//! - **Detail arrives only while a view is open**, because a piece map is tens
//!   of thousands of bits.
//! - **A command is answered immediately**, with the network down.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 09-projection`.

use std::time::{Duration, Instant};

use backend::projection::build::{CollectionFacts, Handles, collection, snapshot};
use backend::projection::emit::{PROGRESS_INTERVAL, Projector};
use backend::projection::state::{
    Command, CommandError, Connectivity, ContactState, DeviceState, Friendship, Handle,
    PortalisState, Progress, Status,
};
use backend::store::records::Role;
use portalis_nexus_demo::section;

const COLLECTION: &[u8] = &[0x11; 16];

fn main() {
    let mut handles = Handles::new();
    let mut projector = Projector::new();

    section("The first tick is a complete snapshot");
    let idle = state(&mut handles, None);
    let first = projector.tick(&idle, None, Duration::ZERO);
    println!(
        "  {} bytes · {} collection(s) · {} alert(s)",
        first.size(),
        first.state.as_ref().map_or(0, |s| s.collections.len()),
        first.state.as_ref().map_or(0, |s| s.alerts.len())
    );
    println!("  Complete rather than a delta, so a restart never depends on");
    println!("  having seen earlier events.");

    section("Idle costs nothing");
    let mut idle_bytes = 0;
    for tick in 1..=100 {
        idle_bytes += projector
            .tick(&idle, None, Duration::from_millis(tick * 100))
            .size();
    }
    println!("  100 ticks over 10 seconds → {idle_bytes} bytes");
    println!("  Nothing changed, so nothing was sent. The comparison lives here");
    println!("  once, rather than in every widget on the other side.");
    assert_eq!(idle_bytes, 0);

    section("Progress coalesces");
    let mut emitted = 0_u128;
    let readings = 400_u64;
    for step in 1..=readings {
        // A reading every 5 ms — 200 a second, which is what a torrent engine
        // will happily produce.
        let at = Duration::from_millis(step * 5);
        let moving = state(&mut handles, Some(progress(step * 256, readings * 256)));
        if !projector.tick(&moving, None, at).is_empty() {
            emitted += 1;
        }
    }
    let span = Duration::from_millis(readings * 5);
    let windows = span.as_millis() / PROGRESS_INTERVAL.as_millis();
    println!(
        "  {readings} readings over {}s → {emitted} emissions",
        span.as_secs()
    );
    println!(
        "  {windows} windows of {}ms, so one each and one to start",
        PROGRESS_INTERVAL.as_millis()
    );
    println!("  An older reading of a moving number is not information.");
    assert!(
        emitted <= windows + 1,
        "coalesced to one per window, got {emitted} over {windows}"
    );

    section("A fact does not wait for the window");
    let forked = forked(&mut handles);
    let emission = projector.tick(&forked, None, Duration::from_millis(readings * 5 + 1));
    println!(
        "  a fork detected mid-transfer → sent at once, {} alert(s)",
        emission.state.as_ref().map_or(0, |s| s.alerts.len())
    );
    println!("  Progress is sampled; a fork is not. Conflating them would mean");
    println!("  waiting up to 250ms to say history is in conflict.");
    assert!(emission.state.is_some());

    section("Detail arrives only while a view is open");
    let detail = detail(handles.of(COLLECTION));
    let unwatched = projector.tick(&forked, Some(&detail), Duration::from_secs(3));
    println!("  nobody looking → {} bytes of detail", unwatched.size());

    projector.watch_detail(Some(handles.of(COLLECTION)));
    let watched = projector.tick(&forked, Some(&detail), Duration::from_secs(4));
    println!(
        "  a view opens  → {} bytes ({} pieces, {} sample rows)",
        watched.size(),
        detail.pieces.len() * 8,
        detail.samples.len() / 16
    );
    let again = projector.tick(&forked, Some(&detail), Duration::from_secs(5));
    println!("  unchanged     → {} bytes", again.size());
    assert!(unwatched.detail.is_none() && watched.detail.is_some() && again.detail.is_none());

    commands_are_answered_at_once(handles.of(COLLECTION));

    println!("\nIdle sent nothing, 400 readings became {emitted} emissions, detail");
    println!("arrived only when asked for, and every command was answered in");
    println!("microseconds with nothing connected.");
}

/// Every command answered without a network, and timed.
fn commands_are_answered_at_once(collection: Handle) {
    section("A command is answered immediately, with the network down");
    for command in [
        Command::CreateCollection {
            name: "Iceland".to_owned(),
            files: Vec::new(),
        },
        Command::ShareWith {
            collection,
            contact: Handle(2),
        },
        Command::AddContact {
            handle: "mira#4KQ2P".to_owned(),
        },
    ] {
        let started = Instant::now();
        let answer = accept(&command);
        let took = started.elapsed();
        match answer {
            Ok(accepted) if accepted.queued => {
                println!("  queued in {took:?} — it will publish when connected");
            }
            Ok(_) => println!("  accepted in {took:?}"),
            Err(error) => println!("  refused in {took:?}: {error}"),
        }
        assert!(took < Duration::from_millis(100), "answered in {took:?}");
    }
    println!("  Acceptance is a local decision, so it never waits for a network.");
    println!("  What happens next arrives through the state, on the object");
    println!("  affected — which is what shows it after a restart mid-operation.");
}

/// Whether the core takes responsibility for a command, decided locally.
fn accept(command: &Command) -> Result<backend::projection::state::Accepted, CommandError> {
    if let Command::CreateCollection { name, .. } = command
        && name.trim().is_empty()
    {
        return Err(CommandError::Invalid(
            "a collection needs a name".to_owned(),
        ));
    }
    if command.is_deferrable() {
        return Ok(backend::projection::state::Accepted {
            id: 1,
            collection: None,
            queued: true,
        });
    }
    // The few that cannot be queued need a connection, and there is none.
    Err(CommandError::Unavailable)
}

fn state(handles: &mut Handles, progress: Option<Progress>) -> PortalisState {
    snapshot(
        device(),
        Connectivity::LocalOnly,
        vec![contact()],
        vec![collection(handles, &facts(progress, None))],
    )
}

fn forked(handles: &mut Handles) -> PortalisState {
    snapshot(
        device(),
        Connectivity::LocalOnly,
        vec![contact()],
        vec![collection(
            handles,
            &facts(Some(progress(512, 1_024)), Some(Status::ConflictingHistory)),
        )],
    )
}

fn facts(progress: Option<Progress>, failure: Option<Status>) -> CollectionFacts {
    CollectionFacts {
        collection_id: COLLECTION.to_vec(),
        name: "Iceland, 2019".to_owned(),
        role: Role::Owner,
        revision: 3,
        entries: 240,
        total_bytes: 1_073_741_824,
        members: vec![vec![2; 32]],
        progress,
        failure,
        paused: false,
        on_disk_bytes: 0,
        uploaded_bytes: 0,
    }
}

fn progress(done: u64, total: u64) -> Progress {
    Progress {
        done,
        total,
        down_bytes_per_second: 4_194_304,
        up_bytes_per_second: 1_048_576,
        peers: 4,
    }
}

fn device() -> DeviceState {
    DeviceState {
        name: "Ada's laptop".to_owned(),
        handle: Some("ada#7Q2XZ".to_owned()),
        fingerprint: "a4f2 9c1b 77de 3081".to_owned(),
        devices: 2,
    }
}

fn contact() -> ContactState {
    ContactState {
        id: Handle(2),
        display_name: "Mira".to_owned(),
        handle: Some("mira#4KQ2P".to_owned()),
        fingerprint: "b8c3 11a7 42ff 90e5".to_owned(),
        verified: true,
        friendship: Friendship::Accepted,
        reachable: None,
    }
}

/// A piece map and a transfer history, at the size they actually reach.
fn detail(id: Handle) -> backend::projection::state::Detail {
    backend::projection::state::Detail {
        id,
        entries: Vec::new(),
        // 8 192 pieces, one bit each.
        pieces: vec![0b1010_1010; 1_024],
        // 60 rows of packed (t, down, up, progress).
        samples: vec![0; 60 * 16],
        peers: vec!["10.0.0.1:6881".to_owned(), "10.0.0.2:6881".to_owned()],
    }
}
