//! What the substrate is doing, attributed back to collections.
//!
//! One task polls the substrate and answers three questions the interface asks
//! continuously: how fast is this moving, who is it moving with, and where did
//! the bytes land. Nothing else in the core talks to the substrate about
//! progress, so there is one place where a reading becomes state and one place
//! where it becomes history.
//!
//! History is written here rather than accumulated by the interface (D8). A
//! chart drawn from what Flutter happened to observe is a second source of
//! truth: it starts when a screen opens, forgets on a restart, and disagrees
//! with the numbers beside it. Written here it is the same history for every
//! screen, and it survives.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;

use crate::nexus::projection::state::{Handle, PortalisState, Status, Transfer};
use crate::nexus::store::Store;
use crate::nexus::store::records::{StoredCollection, StoredSample};
use crate::nexus::substrate::Substrate;
use crate::nexus::torrent::TorrentInfo;

/// How often the substrate is asked. Also the spacing of the history, so a
/// chart's x axis is this constant rather than whatever the interface managed
/// to observe.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// How many readings one collection keeps.
///
/// Half an hour at [`POLL_INTERVAL`]. Bounded because this is a ring for a
/// chart, not an audit log: the oldest reading a person can see is the oldest
/// one worth keeping.
pub const HISTORY_LENGTH: usize = 1800;

/// The most recent reading for each collection, by collection key.
///
/// Held rather than re-fetched because the detail tier needs the same holdings
/// the progress tier just used, and asking the substrate twice would let the
/// two disagree within one frame.
#[derive(Clone, Debug, Default)]
pub struct Holdings(Arc<Mutex<HashMap<Vec<u8>, TorrentInfo>>>);

impl Holdings {
    /// What the substrate last reported for one collection.
    #[must_use]
    pub fn get(&self, collection_key: &[u8]) -> Option<TorrentInfo> {
        self.lock().get(collection_key).cloned()
    }

    fn replace(&self, holdings: HashMap<Vec<u8>, TorrentInfo>) {
        *self.lock() = holdings;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Vec<u8>, TorrentInfo>> {
        // Never held across an await, so poisoning would mean a bug elsewhere;
        // recovering the guard keeps one panicked reader from stopping every
        // later transfer reading.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Polls the substrate until shutdown, publishing state and writing history.
pub(crate) async fn follow_transfers(
    store: Arc<Store>,
    states: watch::Sender<PortalisState>,
    collections: Arc<Mutex<super::nexus::LocalCollections>>,
    substrate: Arc<dyn Substrate>,
    holdings: Holdings,
    mut shutdown: super::supervisor::Shutdown,
    details: super::nexus::DetailSources,
) {
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // The last reading written for each collection, and when — so an
    // unchanged one can be recognised without reading back what was just
    // stored, and so a rate can be measured against real elapsed time.
    let mut written: HashMap<Vec<u8>, LastReading> = HashMap::new();
    loop {
        tokio::select! {
            () = shutdown.requested() => return,
            _ = tick.tick() => {}
        }

        let reported = substrate.holdings().await;
        let by_handle: HashMap<&str, &TorrentInfo> = reported
            .iter()
            .map(|info| (info.info_hash.as_str(), info))
            .collect();

        let carried = match carried_collections(&store) {
            Ok(carried) => carried,
            Err(error) => {
                crate::nexus::log::clog!("nexus", "could not read carried collections: {error}");
                continue;
            }
        };

        // A torrent the engine is carrying that no collection claims is an
        // orphan: a deleted collection whose download was never stopped. It
        // is what let the interface report an active transfer with nothing
        // on screen to explain it, so it is released here rather than left
        // for somebody to notice.
        let claimed: std::collections::HashSet<&str> = carried
            .iter()
            .map(|(_, handle, _)| handle.as_str())
            .collect();
        for info in &reported {
            if !claimed.contains(info.info_hash.as_str())
                && let Err(error) = substrate.release(&info.info_hash).await
            {
                crate::nexus::log::clog!(
                    "nexus",
                    "could not release an unclaimed torrent: {error}"
                );
            }
        }

        let mut current = HashMap::new();
        for (key, handle, paused) in carried {
            let Some(info) = by_handle.get(handle.as_str()) else {
                continue;
            };
            // A torrent still checking itself has no progress to report, and
            // recording the placeholder zero put a false restart in the middle
            // of the chart every time the app reopened.
            let rates = measured_rates(info, written.get(&key));
            if info.knows_progress() {
                mark_moments(&store, &key, info);
                let now = unix_time_ns();
                let sample = record(
                    &store,
                    &key,
                    info,
                    rates,
                    written.get(&key).map(|previous| &previous.sample),
                );
                written.insert(
                    key.clone(),
                    LastReading {
                        sample,
                        at: now,
                        uploaded: info.uploaded_bytes,
                    },
                );
            }
            current.insert(key.clone(), (*info).clone());

            let projected = collections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .handle(&key);
            if let Some(projected) = projected {
                publish(&states, projected, info, rates, paused);
            }
        }
        // A collection that stopped being carried must not keep its last
        // reading here, or re-adding it would compare against a stale one and
        // drop the first sample of the new transfer.
        written.retain(|key, _| current.contains_key(key));
        // Replaced before refreshing, so a detail rebuilt below reads this
        // tick's holdings rather than the last one's.
        holdings.replace(current);
        // The detail tier is where peers, the piece map and per-file progress
        // live. Without this an open collection shows whatever was true when
        // it was opened and never moves, which is what made a running
        // transfer look frozen.
        for handle in details.watched() {
            details.refresh(handle);
        }
    }
}

/// Every collection something is currently carrying, with its pause flag.
fn carried_collections(
    store: &Store,
) -> Result<Vec<(Vec<u8>, String, bool)>, crate::nexus::store::StoreError> {
    Ok(store
        .collections()?
        .into_iter()
        .filter_map(|(key, stored)| {
            stored
                .substrate_handle
                .map(|handle| (key, handle, stored.paused))
        })
        .collect())
}

/// Records when this collection began moving and when it finished.
///
/// Written once each, the moment the engine reports it, and never revised.
/// The interface used to answer "completed in" by measuring the span of the
/// surviving transfer history — which is a measurement of the ring, not of the
/// transfer, and read a delete-and-re-add as one download lasting six minutes
/// when it had been two of half a minute.
///
/// A failure is logged and dropped: losing the moment is not a reason to stop
/// reporting the transfer it belongs to.
fn mark_moments(store: &Store, key: &[u8], info: &TorrentInfo) {
    let Ok(Some(stored)) = store.collection(key) else {
        return;
    };
    // Bytes are what starts a transfer, not the decision to allow one — a
    // collection queued behind a dead swarm has not started.
    let starting = stored.started_at.is_none() && info.progress_bytes > 0;
    let finishing = stored.completed_at.is_none() && info.finished;
    if !starting && !finishing {
        return;
    }
    let now = unix_time_ns();
    let updated = StoredCollection {
        started_at: stored.started_at.or((starting).then_some(now)),
        // A collection that was already complete when this device first saw it
        // still needs a start, or its duration is unanswerable rather than
        // zero. Both land on the same tick, which reads as "instant" — true
        // enough for something that was never fetched.
        completed_at: stored.completed_at.or((finishing).then_some(now)),
        ..stored
    };
    if let Err(error) = store.put_collection(key, &updated) {
        crate::nexus::log::clog!("nexus", "could not record a transfer moment: {error}");
    }
}

/// Writes one reading to the ring, trimming it back to its bound.
///
/// A reading identical to the one before it is dropped rather than written. A
/// finished collection reports the same numbers every second forever, and
/// recording them extended its chart for as long as the app stayed open — the
/// transfer was over, the graph kept growing. Writing only what changed keeps
/// the final zero (so a chart shows the transfer ramp down and stop) without
/// keeping the silence after it.
///
/// A failure is logged and dropped rather than propagated: losing a point of a
/// chart is not a reason to stop reporting the transfer it describes.
fn record(
    store: &Store,
    key: &[u8],
    info: &TorrentInfo,
    rates: Rates,
    last: Option<&StoredSample>,
) -> StoredSample {
    let sample = StoredSample {
        done: info.progress_bytes,
        total: info.total_bytes,
        down_bytes_per_second: rates.down,
        up_bytes_per_second: rates.up,
        peers: u16::try_from(info.live_peers).unwrap_or(u16::MAX),
    };
    if last == Some(&sample) {
        return sample;
    }
    if let Err(error) = store.put_sample(key, unix_time_ns(), &sample) {
        crate::nexus::log::clog!("nexus", "could not record a transfer sample: {error}");
        return sample;
    }
    if let Err(error) = store.trim_samples(key, HISTORY_LENGTH) {
        crate::nexus::log::clog!("nexus", "could not trim the transfer history: {error}");
    }
    sample
}

/// Updates one collection's progress tier from one reading.
fn publish(
    states: &watch::Sender<PortalisState>,
    handle: Handle,
    info: &TorrentInfo,
    rates: Rates,
    paused: bool,
) {
    let transfer = transfer_of(info, rates);
    let status = status_of(info, paused);
    states.send_if_modified(|state| {
        let Some(collection) = state
            .collections
            .iter_mut()
            .find(|collection| collection.id == handle)
        else {
            return false;
        };
        // Compared rather than assigned: a reading identical to the last one
        // is not a change, and waking every watcher once a second to tell them
        // nothing is what makes an idle app warm.
        if collection.transfer == transfer
            && collection.status == status
            && collection.on_disk_bytes == info.progress_bytes
            && collection.total_bytes == info.total_bytes
            && collection.uploaded_bytes == info.uploaded_bytes
        {
            return false;
        }
        collection.transfer = transfer;
        collection.status = status;
        collection.on_disk_bytes = info.progress_bytes;
        // The same reading the fraction is measured against. It used to be
        // summed from the stored descriptor by a different worker on a
        // different schedule, so the percentage and the "x of y" beside it had
        // two denominators from two sources — nothing kept them equal, and a
        // stale one showed 100% next to less than the whole.
        collection.total_bytes = info.total_bytes;
        collection.uploaded_bytes = info.uploaded_bytes;
        true
    });
}

/// The progress tier for one reading, or `None` once nothing is moving.
///
/// "Finished" is not the same as "idle". A collection this device has every
/// byte of is still moving while somebody is pulling it, and that is exactly
/// when an owner wants to see the numbers — hiding the tier the moment a
/// download completes is what makes seeding look like nothing happening.
fn transfer_of(info: &TorrentInfo, rates: Rates) -> Option<Transfer> {
    let idle = info.finished && info.live_peers == 0 && rates.up == 0;
    if idle {
        return None;
    }
    let fraction = if info.total_bytes == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss, reason = "a fraction for a progress bar")]
        {
            info.progress_bytes as f32 / info.total_bytes as f32
        }
    };
    let down = rates.down;
    // The engine decides when it is done, not the ratio. Bytes can be all
    // present while pieces are still being verified, and a torrent resumed
    // over existing files counts what is on disk before it knows whether any
    // of it is wanted — either way the fraction reaches one while the engine
    // is still working, and a bar reading 100% beside the word Downloading is
    // the interface contradicting itself.
    let complete = if info.finished {
        1.0
    } else {
        fraction.min(0.9999)
    };
    Some(Transfer {
        progress: complete.clamp(0.0, 1.0),
        down_bytes_per_second: down,
        up_bytes_per_second: rates.up,
        peers: u16::try_from(info.live_peers).unwrap_or(u16::MAX),
        // Honest about not knowing: no rate means no estimate rather than an
        // infinite one.
        eta_secs: (down > 0 && info.total_bytes > info.progress_bytes).then(|| {
            let remaining = info.total_bytes - info.progress_bytes;
            u32::try_from(remaining / u64::from(down)).unwrap_or(u32::MAX)
        }),
    })
}

/// What one reading says the collection is doing.
///
/// A person's pause outranks the numbers, exactly as it does in the projection
/// builder — the two have to agree, because they answer the same question for
/// the same screen.
/// The poller is the one caller with a live reading, which is what makes its
/// answer the most informed one there is — see `status_for`.
fn status_of(info: &TorrentInfo, paused: bool) -> Status {
    crate::nexus::projection::state::status_for(crate::nexus::projection::state::StatusFacts {
        draft: false,
        paused,
        carried: true,
        publishing: false,
        importing: false,
        live: Some(info),
    })
}

/// What actually moved since the last reading, per second.
///
/// Measured from the byte counters rather than taken from the engine's own
/// figure, which is a smoothed average: after the last byte of a one-second
/// download it went on reporting five megabytes a second, then three, then
/// two, decaying to nothing over six seconds. Every one of those readings
/// said bytes were moving while `progress_bytes` did not change once — a
/// chart of a transfer that was already over, and a rate beside it claiming
/// throughput that no longer existed.
///
/// Bytes are the one number that cannot be smoothed into saying something
/// untrue. When nothing arrives, this is exactly zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rates {
    pub down: u32,
    pub up: u32,
}

/// What was last written for one collection, and the counters it was written
/// against — a rate is a difference, so it needs both ends.
#[derive(Clone)]
struct LastReading {
    sample: StoredSample,
    at: u64,
    uploaded: u64,
}

#[cfg(test)]
impl LastReading {
    /// The same reading, as though it had counted `done` bytes.
    fn starting_from(&self, done: u64) -> Self {
        let mut copy = self.clone();
        copy.sample.done = done;
        copy
    }
}

fn measured_rates(info: &TorrentInfo, last: Option<&LastReading>) -> Rates {
    // Nothing to measure against yet. Claiming the engine's average here
    // would put a spike at the start of every chart that no bytes justify.
    let Some(previous) = last else {
        return Rates::default();
    };
    let elapsed = unix_time_ns().saturating_sub(previous.at);
    if elapsed == 0 {
        return Rates::default();
    }
    Rates {
        down: rate(
            info.progress_bytes.saturating_sub(previous.sample.done),
            elapsed,
        ),
        up: rate(
            info.uploaded_bytes.saturating_sub(previous.uploaded),
            elapsed,
        ),
    }
}

/// Bytes over nanoseconds, as whole bytes per second.
fn rate(bytes: u64, elapsed_ns: u64) -> u32 {
    let per_second = bytes.saturating_mul(1_000_000_000) / elapsed_ns;
    u32::try_from(per_second).unwrap_or(u32::MAX)
}

/// Now, as the store and the projection both count it.
pub(crate) fn unix_time_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(progress: u64, total: u64, finished: bool) -> TorrentInfo {
        TorrentInfo {
            id: 1,
            info_hash: "a1b2".to_owned(),
            name: "Iceland".to_owned(),
            state: "live".to_owned(),
            progress_bytes: progress,
            total_bytes: total,
            uploaded_bytes: 0,
            finished,
            error: None,
            files: Vec::new(),
            live_peers: 3,
            live_peer_addrs: vec!["10.0.0.1:6881".to_owned()],
        }
    }

    /// A torrent being verified reports zero progress by design — the scan
    /// cursor would climb and then collapse, which is worse. Recording that
    /// zero put a false restart between two real readings, and the chart drew
    /// one transfer as two with a dead flat stretch between them.
    #[test]
    fn a_torrent_still_checking_itself_has_no_reading_to_record() {
        let mut checking = info(0, 100, false);
        checking.state = "Initializing".to_owned();
        assert!(!checking.knows_progress());

        let mut live = info(40, 100, false);
        live.state = "live".to_owned();
        assert!(live.knows_progress());
    }

    /// The engine's own figure is a smoothed average. After the last byte of a
    /// one-second download it went on reporting five megabytes a second, then
    /// three, then two, decaying to nothing over six more seconds — six
    /// readings claiming throughput while the byte counter did not move once.
    /// Bytes cannot be smoothed into saying something untrue.
    #[test]
    fn a_rate_is_what_moved_not_what_the_engine_averaged() {
        let arrived = info(56_070_710, 56_070_710, true);

        let a_second_ago = LastReading {
            sample: StoredSample {
                done: 56_070_710,
                total: 56_070_710,
                down_bytes_per_second: 5_686_154,
                up_bytes_per_second: 0,
                peers: 3,
            },
            at: unix_time_ns().saturating_sub(1_000_000_000),
            uploaded: 0,
        };

        assert_eq!(
            measured_rates(&arrived, Some(&a_second_ago)),
            Rates::default(),
            "nothing arrived, so nothing was moving"
        );

        // And what did arrive is measured against the time it took.
        let moving = info(2_000_000, 56_070_710, false);
        let measured = measured_rates(&moving, Some(&a_second_ago.starting_from(0)));
        assert!(
            (1_900_000..=2_100_000).contains(&measured.down),
            "about two megabytes in about a second, got {}",
            measured.down
        );
    }

    /// Written when they happen, and never again. A duration measured
    /// afterwards from surviving history measures the ring rather than the
    /// transfer — which is how a delete and a re-add read as one six-minute
    /// download instead of two of half a minute.
    #[test]
    fn the_moments_are_recorded_once_and_not_revised() {
        let dir = std::env::temp_dir().join(format!(
            "portalis-moments-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let store = Store::open(dir.join("portalis.redb")).expect("opens");
        store
            .put_collection(b"a", &collection_row())
            .expect("writes");

        // Nothing has moved: neither moment has arrived.
        mark_moments(&store, b"a", &info(0, 100, false));
        let row = store.collection(b"a").expect("reads").expect("exists");
        assert_eq!(row.started_at, None);
        assert_eq!(row.completed_at, None);

        // The first byte starts it.
        mark_moments(&store, b"a", &info(10, 100, false));
        let started = store
            .collection(b"a")
            .expect("reads")
            .expect("exists")
            .started_at
            .expect("a start");

        // Later readings do not move a start that already happened.
        mark_moments(&store, b"a", &info(50, 100, false));
        assert_eq!(
            store
                .collection(b"a")
                .expect("reads")
                .expect("exists")
                .started_at,
            Some(started),
            "a start is when it started, not when it was last seen running"
        );

        // The engine saying so is what completes it.
        mark_moments(&store, b"a", &info(100, 100, true));
        let completed = store
            .collection(b"a")
            .expect("reads")
            .expect("exists")
            .completed_at
            .expect("an end");
        assert!(completed >= started);

        // And still seeding afterwards does not re-complete it.
        mark_moments(&store, b"a", &info(100, 100, true));
        assert_eq!(
            store
                .collection(b"a")
                .expect("reads")
                .expect("exists")
                .completed_at,
            Some(completed),
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    fn collection_row() -> StoredCollection {
        StoredCollection {
            name: "Iceland".to_owned(),
            role: crate::nexus::store::records::Role::Owner,
            content_key: [0; 32],
            media_path: String::new(),
            sources: Vec::new(),
            paused: false,
            on_disk_bytes: 0,
            substrate_handle: Some("abc".to_owned()),
            draft: false,
            started_at: None,
            completed_at: None,
        }
    }

    /// One hundred percent is a claim about the engine, not about arithmetic.
    ///
    /// Bytes can all be present while pieces are still being verified, and a
    /// torrent resumed over existing files counts what is on disk before it
    /// knows whether any of it was wanted. Both make the ratio reach one while
    /// the engine is still working, and a bar reading 100% beside the word
    /// Downloading is the interface contradicting itself.
    #[test]
    fn progress_does_not_reach_the_end_before_the_engine_says_so() {
        let every_byte = info(100, 100, false);
        let transfer = transfer_of(&every_byte, Rates::default()).expect("still moving");
        assert!(
            transfer.progress < 1.0,
            "unfinished cannot report complete, got {}",
            transfer.progress
        );

        // And once it does say so, nothing holds it back.
        let mut done = info(100, 100, true);
        done.live_peers = 1;
        assert_eq!(
            transfer_of(&done, Rates::default())
                .expect("still serving")
                .progress,
            1.0,
            "finished is finished"
        );
    }

    /// A finished collection reports identical numbers every second for as
    /// long as the app stays open. Recording them grew the chart of a transfer
    /// that had already ended, so a reading equal to the one before it is
    /// dropped — while the first reading that differs is always kept, which is
    /// what leaves the ramp down to zero visible.
    #[test]
    fn an_unchanged_reading_is_not_recorded_again() {
        let dir = std::env::temp_dir().join(format!(
            "portalis-samples-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let store = Store::open(dir.join("portalis.redb")).expect("opens");

        let moving = info(10, 100, false);
        let first = record(&store, b"a", &moving, Rates { down: 1, up: 0 }, None);
        assert_eq!(store.samples(b"a").expect("reads").len(), 1);

        // The same reading again says nothing the first one did not.
        let second = record(
            &store,
            b"a",
            &moving,
            Rates { down: 1, up: 0 },
            Some(&first),
        );
        assert_eq!(store.samples(b"a").expect("reads").len(), 1);

        // A reading that differs is written, so the chart still shows the
        // transfer finishing rather than stopping mid-flight.
        let done = info(100, 100, true);
        record(&store, b"a", &done, Rates::default(), Some(&second));
        assert_eq!(store.samples(b"a").expect("reads").len(), 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Finished and idle is nothing to report; finished and still being pulled
    /// is seeding, which is the half an owner most wants to see.
    #[test]
    fn a_finished_transfer_reports_nothing_only_once_it_is_also_idle() {
        let mut done = info(100, 100, true);
        done.live_peers = 0;
        assert_eq!(transfer_of(&done, Rates::default()), None);

        // Serving somebody: measured upload, so it is genuinely not idle.
        let seeding = TorrentInfo {
            live_peers: 2,
            ..done.clone()
        };
        let uploading = Rates {
            down: 0,
            up: 500_000,
        };
        let seeding = transfer_of(&seeding, uploading).expect("still moving");
        assert!((seeding.progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(seeding.up_bytes_per_second, 500_000);
        assert_eq!(seeding.eta_secs, None, "nothing left to arrive");

        let downloading = Rates {
            down: 1_000_000,
            up: 0,
        };
        let moving = transfer_of(&info(50, 100, false), downloading).expect("still moving");
        assert!((moving.progress - 0.5).abs() < f32::EPSILON);
        assert_eq!(moving.peers, 3);
        assert_eq!(moving.eta_secs, Some(50 / 1_000_000));
    }

    /// A total of zero is a torrent whose metadata has not arrived. Dividing by
    /// it would be a progress bar full of NaN.
    #[test]
    fn a_transfer_with_no_known_total_reports_no_progress_rather_than_nan() {
        let unknown = transfer_of(&info(0, 0, false), Rates::default()).expect("carried");

        assert!((unknown.progress - 0.0).abs() < f32::EPSILON);
        assert_eq!(unknown.eta_secs, None, "nothing to estimate from");
    }

    /// The same rule as the projection builder: a person's choice outranks the
    /// numbers, and both places have to say so or one screen contradicts the
    /// other.
    #[test]
    fn a_pause_outranks_whatever_the_numbers_are_doing() {
        assert_eq!(status_of(&info(50, 100, false), true), Status::Paused);
        assert_eq!(status_of(&info(50, 100, false), false), Status::Downloading);
        assert_eq!(status_of(&info(0, 100, false), false), Status::Preparing);
        assert_eq!(status_of(&info(100, 100, true), false), Status::Available);
        assert_eq!(
            status_of(&info(100, 100, true), true),
            Status::Paused,
            "a finished collection a person paused is still paused"
        );
    }

    #[test]
    fn holdings_answer_with_the_last_reading_and_nothing_for_the_rest() {
        let holdings = Holdings::default();
        assert!(holdings.get(b"unknown").is_none());

        holdings.replace(HashMap::from([(b"key".to_vec(), info(1, 2, false))]));

        assert_eq!(
            holdings.get(b"key").map(|info| info.progress_bytes),
            Some(1)
        );
        assert!(holdings.get(b"other").is_none());
    }
}
