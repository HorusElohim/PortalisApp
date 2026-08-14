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

use crate::projection::state::{Handle, PortalisState, Status, Transfer};
use crate::store::records::StoredSample;
use crate::store::Store;
use crate::substrate::Substrate;
use crate::torrent::TorrentInfo;

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
                crate::log::clog!("nexus", "could not read carried collections: {error}");
                continue;
            }
        };

        let mut current = HashMap::new();
        for (key, handle, paused) in carried {
            let Some(info) = by_handle.get(handle.as_str()) else {
                continue;
            };
            record(&store, &key, info);
            current.insert(key.clone(), (*info).clone());

            let projected = collections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .handle(&key);
            if let Some(projected) = projected {
                publish(&states, projected, info, paused);
            }
        }
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
) -> Result<Vec<(Vec<u8>, String, bool)>, crate::store::StoreError> {
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

/// Writes one reading to the ring, trimming it back to its bound.
///
/// A failure is logged and dropped rather than propagated: losing a point of a
/// chart is not a reason to stop reporting the transfer it describes.
fn record(store: &Store, key: &[u8], info: &TorrentInfo) {
    let sample = StoredSample {
        done: info.progress_bytes,
        total: info.total_bytes,
        down_bytes_per_second: per_second(info.download_mbps),
        up_bytes_per_second: per_second(info.upload_mbps),
        peers: u16::try_from(info.live_peers).unwrap_or(u16::MAX),
    };
    if let Err(error) = store.put_sample(key, unix_time_ns(), &sample) {
        crate::log::clog!("nexus", "could not record a transfer sample: {error}");
        return;
    }
    if let Err(error) = store.trim_samples(key, HISTORY_LENGTH) {
        crate::log::clog!("nexus", "could not trim the transfer history: {error}");
    }
}

/// Updates one collection's progress tier from one reading.
fn publish(
    states: &watch::Sender<PortalisState>,
    handle: Handle,
    info: &TorrentInfo,
    paused: bool,
) {
    let transfer = transfer_of(info);
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
        {
            return false;
        }
        collection.transfer = transfer;
        collection.status = status;
        collection.on_disk_bytes = info.progress_bytes;
        true
    });
}

/// The progress tier for one reading, or `None` once nothing is moving.
///
/// "Finished" is not the same as "idle". A collection this device has every
/// byte of is still moving while somebody is pulling it, and that is exactly
/// when an owner wants to see the numbers — hiding the tier the moment a
/// download completes is what makes seeding look like nothing happening.
fn transfer_of(info: &TorrentInfo) -> Option<Transfer> {
    let idle = info.finished && info.live_peers == 0 && info.upload_mbps <= 0.0;
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
    let down = per_second(info.download_mbps);
    Some(Transfer {
        progress: fraction.clamp(0.0, 1.0),
        down_bytes_per_second: down,
        up_bytes_per_second: per_second(info.upload_mbps),
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
fn status_of(info: &TorrentInfo, paused: bool) -> Status {
    if paused {
        Status::Paused
    } else if info.finished {
        Status::Available
    } else if info.progress_bytes == 0 {
        Status::Preparing
    } else {
        Status::Downloading
    }
}

/// Megabits per second, as the bytes per second everything else speaks.
fn per_second(mbps: f64) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a display rate, clamped below"
    )]
    {
        (mbps * 125_000.0).max(0.0).min(f64::from(u32::MAX)) as u32
    }
}

fn unix_time_ns() -> u64 {
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
            download_mbps: 8.0,
            upload_mbps: 0.0,
            finished,
            error: None,
            files: Vec::new(),
            live_peers: 3,
            live_peer_addrs: vec!["10.0.0.1:6881".to_owned()],
        }
    }

    /// Megabits are what the substrate reports and bytes are what everything
    /// else speaks, so the conversion happens once, here.
    #[test]
    fn a_rate_crosses_from_megabits_to_bytes_without_overflowing() {
        assert_eq!(per_second(8.0), 1_000_000);
        assert_eq!(per_second(0.0), 0);
        assert_eq!(per_second(-1.0), 0, "a negative rate is no rate");
        assert_eq!(
            per_second(f64::MAX),
            u32::MAX,
            "clamped rather than wrapped"
        );
    }

    /// Finished and idle is nothing to report; finished and still being pulled
    /// is seeding, which is the half an owner most wants to see.
    #[test]
    fn a_finished_transfer_reports_nothing_only_once_it_is_also_idle() {
        let mut done = info(100, 100, true);
        done.live_peers = 0;
        done.upload_mbps = 0.0;
        assert_eq!(transfer_of(&done), None);

        let seeding = TorrentInfo {
            live_peers: 2,
            upload_mbps: 4.0,
            ..done.clone()
        };
        let seeding = transfer_of(&seeding).expect("still moving");
        assert!((seeding.progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(seeding.up_bytes_per_second, per_second(4.0));
        assert_eq!(seeding.eta_secs, None, "nothing left to arrive");

        let moving = transfer_of(&info(50, 100, false)).expect("still moving");
        assert!((moving.progress - 0.5).abs() < f32::EPSILON);
        assert_eq!(moving.peers, 3);
        assert_eq!(moving.eta_secs, Some(50 / 1_000_000));
    }

    /// A total of zero is a torrent whose metadata has not arrived. Dividing by
    /// it would be a progress bar full of NaN.
    #[test]
    fn a_transfer_with_no_known_total_reports_no_progress_rather_than_nan() {
        let unknown = transfer_of(&info(0, 0, false)).expect("carried");

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
