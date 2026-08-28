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

use crate::nexus::projection::state::{Handle, PeerState, PortalisState, Status, Transfer};
use crate::nexus::store::Store;
use crate::nexus::store::records::{StoredCollection, StoredPeerHistory, StoredSample};
use crate::nexus::substrate::Substrate;
use crate::nexus::torrent::TorrentInfo;

/// How often the substrate is asked. Also the spacing of the history, so a
/// chart's x axis is this constant rather than whatever the interface managed
/// to observe.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How many readings one collection keeps.
///
/// Half an hour at [`POLL_INTERVAL`]. Bounded because this is a ring for a
/// chart, not an audit log: the oldest reading a person can see is the oldest
/// one worth keeping.
pub const HISTORY_LENGTH: usize = 3600;

/// The most recent reading for each collection, by collection key.
///
/// Held rather than re-fetched because the detail tier needs the same holdings
/// the progress tier just used, and asking the substrate twice would let the
/// two disagree within one frame.
#[derive(Clone, Debug, Default)]
pub struct Holdings(Arc<Mutex<HashMap<Vec<u8>, Holding>>>);

/// One collection's last reading, with the peer rates measured alongside it.
///
/// The rates live here rather than on [`TorrentInfo`] because they are not
/// something the substrate reports: they are a difference between two polls,
/// and only this task knows both.
#[derive(Clone, Debug)]
pub struct Holding {
    pub info: TorrentInfo,
    pub peers: Vec<crate::nexus::projection::state::PeerState>,
}

impl Holdings {
    /// What the substrate last reported for one collection.
    #[must_use]
    pub fn get(&self, collection_key: &[u8]) -> Option<TorrentInfo> {
        self.lock()
            .get(collection_key)
            .map(|holding| holding.info.clone())
    }

    /// The peers of one collection, with the rates measured for this tick.
    #[must_use]
    pub fn peers(&self, collection_key: &[u8]) -> Vec<crate::nexus::projection::state::PeerState> {
        self.lock()
            .get(collection_key)
            .map(|holding| holding.peers.clone())
            .unwrap_or_default()
    }

    fn replace(&self, holdings: HashMap<Vec<u8>, Holding>) {
        *self.lock() = holdings;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Vec<u8>, Holding>> {
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
    let epoch = unix_time_ns();
    let mut peer_ledgers: HashMap<Vec<u8>, HashMap<PeerKey, StoredPeerHistory>> = HashMap::new();
    let mut last_current: HashMap<Vec<u8>, Holding> = HashMap::new();
    loop {
        tokio::select! {
            () = shutdown.requested() => {
                snapshot_holdings(&store, &last_current, &mut peer_ledgers, epoch);
                return;
            },
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
            .map(|collection| collection.handle.as_str())
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
        for CarriedCollection {
            key,
            handle,
            paused,
            local_source,
        } in carried
        {
            let Some(info) = by_handle.get(handle.as_str()) else {
                continue;
            };
            // A torrent still checking itself has no *payload* progress to
            // record: its scan cursor would put a false restart in the middle
            // of the receive chart. The live tier still exposes that cursor to
            // an owner as explicitly-labelled source verification.
            let measured = measured_rates(info, written.get(&key));
            // Peer rates share the collection's measurement window, so a row
            // and the total above it are differences over the same interval
            // rather than two numbers from two clocks.
            let elapsed = written
                .get(&key)
                .map_or(0, |previous| unix_time_ns().saturating_sub(previous.at));
            let peers = effective_peers(
                &store,
                &key,
                measured_peers(info, written.get(&key), elapsed),
                &mut peer_ledgers,
                epoch,
            );
            // Reads performed by ReferencedStorage while an owner torrent is
            // being checked are local source I/O. Keep the rate visible, but
            // carry the explicit source_reading marker so the UI does not call
            // it a download.
            let rates = measured;
            if info.knows_progress() && (!local_source || rates.down > 0 || rates.up > 0) {
                if !local_source && mark_moments(&store, &key, info) {
                    snapshot_peers(&store, &key, info, &peers, &mut peer_ledgers, epoch);
                }
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
                        fetched: info.fetched_bytes,
                        uploaded: info.uploaded_bytes,
                        peers: peer_counters(info),
                    },
                );
            }
            #[cfg(target_os = "ios")]
            if let Err(error) =
                crate::nexus::torrent::move_completed_import_entries(&store, &key, info).await
            {
                crate::nexus::log::clog!(
                    "torrent",
                    "could not move verified received media into Photos: {error:#}"
                );
            }
            current.insert(
                key.clone(),
                Holding {
                    info: (*info).clone(),
                    peers,
                },
            );

            let projected = collections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .handle(&key);
            if let Some(projected) = projected {
                publish(&states, projected, info, rates, paused, local_source);
            }
        }
        // A collection that stopped being carried must not keep its last
        // reading here, or re-adding it would compare against a stale one and
        // drop the first sample of the new transfer.
        written.retain(|key, _| current.contains_key(key));
        // Replaced before refreshing, so a detail rebuilt below reads this
        // tick's holdings rather than the last one's.
        holdings.replace(current);
        last_current = holdings.lock().clone();
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
struct CarriedCollection {
    key: Vec<u8>,
    handle: String,
    paused: bool,
    local_source: bool,
}

fn carried_collections(
    store: &Store,
) -> Result<Vec<CarriedCollection>, crate::nexus::store::StoreError> {
    Ok(store
        .collections()?
        .into_iter()
        .filter_map(|(key, stored)| {
            stored.substrate_handle.map(|handle| CarriedCollection {
                key,
                handle,
                paused: stored
                    .lifecycle
                    .activity()
                    .is_some_and(crate::nexus::store::records::StoredActivity::is_paused),
                local_source: !stored.sources.is_empty(),
            })
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
fn mark_moments(store: &Store, key: &[u8], info: &TorrentInfo) -> bool {
    let Ok(Some(stored)) = store.collection(key) else {
        return false;
    };
    // Bytes are what starts a transfer, not the decision to allow one — a
    // collection queued behind a dead swarm has not started.
    let starting = stored.started_at.is_none() && info.progress_bytes > 0;
    let finishing = stored.completed_at.is_none() && info.finished;
    if !starting && !finishing {
        return false;
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
    finishing
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PeerKey {
    address: String,
    client: Option<String>,
}

impl PeerKey {
    fn of(peer: &PeerState) -> Self {
        Self {
            address: peer.address.clone(),
            client: peer.client.clone(),
        }
    }
}

fn effective_peers(
    store: &Store,
    collection: &[u8],
    mut peers: Vec<PeerState>,
    ledgers: &mut HashMap<Vec<u8>, HashMap<PeerKey, StoredPeerHistory>>,
    epoch: u64,
) -> Vec<PeerState> {
    let ledger = ledgers.entry(collection.to_vec()).or_insert_with(|| {
        store
            .peer_history(collection)
            .unwrap_or_else(|error| {
                crate::nexus::log::clog!("nexus", "could not read peer history: {error}");
                Vec::new()
            })
            .into_iter()
            .map(|peer| {
                (
                    PeerKey {
                        address: peer.address.clone(),
                        client: peer.client.clone(),
                    },
                    peer,
                )
            })
            .collect()
    });
    for peer in &mut peers {
        let Some(saved) = ledger.get(&PeerKey::of(peer)) else {
            continue;
        };
        peer.down_bytes = saved.total_down_bytes.saturating_add(unsaved(
            peer.down_bytes,
            saved.checkpoint_down_bytes,
            saved.checkpoint_epoch,
            epoch,
        ));
        peer.up_bytes = saved.total_up_bytes.saturating_add(unsaved(
            peer.up_bytes,
            saved.checkpoint_up_bytes,
            saved.checkpoint_epoch,
            epoch,
        ));
    }
    peers
}

fn unsaved(raw: u64, checkpoint: u64, checkpoint_epoch: u64, epoch: u64) -> u64 {
    if checkpoint_epoch == epoch && raw >= checkpoint {
        raw - checkpoint
    } else {
        raw
    }
}

fn snapshot_holdings(
    store: &Store,
    holdings: &HashMap<Vec<u8>, Holding>,
    ledgers: &mut HashMap<Vec<u8>, HashMap<PeerKey, StoredPeerHistory>>,
    epoch: u64,
) {
    for (key, holding) in holdings {
        snapshot_peers(store, key, &holding.info, &holding.peers, ledgers, epoch);
    }
}

fn snapshot_peers(
    store: &Store,
    collection: &[u8],
    info: &TorrentInfo,
    peers: &[PeerState],
    ledgers: &mut HashMap<Vec<u8>, HashMap<PeerKey, StoredPeerHistory>>,
    epoch: u64,
) {
    let now = unix_time_ns();
    let ledger = ledgers.entry(collection.to_vec()).or_default();
    for raw in &info.live_peer_addrs {
        let key = PeerKey {
            address: raw.address.clone(),
            client: raw.client.clone(),
        };
        let rates = peers.iter().find(|peer| PeerKey::of(peer) == key);
        let peer = ledger
            .entry(key.clone())
            .or_insert_with(|| StoredPeerHistory {
                address: key.address.clone(),
                client: key.client.clone(),
                first_seen_at: now,
                last_seen_at: now,
                total_down_bytes: 0,
                total_up_bytes: 0,
                checkpoint_down_bytes: 0,
                checkpoint_up_bytes: 0,
                checkpoint_epoch: 0,
                last_down_bytes_per_second: 0,
                last_up_bytes_per_second: 0,
            });
        peer.total_down_bytes = peer.total_down_bytes.saturating_add(unsaved(
            raw.fetched_bytes,
            peer.checkpoint_down_bytes,
            peer.checkpoint_epoch,
            epoch,
        ));
        peer.total_up_bytes = peer.total_up_bytes.saturating_add(unsaved(
            raw.uploaded_bytes,
            peer.checkpoint_up_bytes,
            peer.checkpoint_epoch,
            epoch,
        ));
        peer.checkpoint_down_bytes = raw.fetched_bytes;
        peer.checkpoint_up_bytes = raw.uploaded_bytes;
        peer.checkpoint_epoch = epoch;
        peer.last_seen_at = now;
        peer.last_down_bytes_per_second = rates.map_or(0, |peer| peer.down_bytes_per_second);
        peer.last_up_bytes_per_second = rates.map_or(0, |peer| peer.up_bytes_per_second);
        if let Err(error) = store.put_peer_history(collection, peer) {
            crate::nexus::log::clog!("nexus", "could not snapshot peer history: {error}");
        }
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
    local_source: bool,
) {
    let transfer = transfer_of_for(info, rates, local_source);
    let status = status_of_for(info, paused, local_source);
    let on_disk_bytes = info.progress_bytes;
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
            && collection.on_disk_bytes == on_disk_bytes
            && collection.total_bytes == info.total_bytes
            && collection.uploaded_bytes == info.uploaded_bytes
        {
            return false;
        }
        collection.transfer = transfer;
        collection.status = status;
        collection.on_disk_bytes = on_disk_bytes;
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
fn transfer_of_for(info: &TorrentInfo, rates: Rates, local_source: bool) -> Option<Transfer> {
    let source_checking = local_source && info.source_check_bytes.is_some();
    let idle = if local_source {
        !source_checking
            && info.finished
            && info.live_peers == 0
            && rates.down == 0
            && rates.up == 0
    } else {
        info.finished && info.live_peers == 0 && rates.up == 0
    };
    if idle {
        return None;
    }
    let progressed_bytes = if source_checking {
        info.source_check_bytes.unwrap_or(0)
    } else {
        info.progress_bytes
    };
    let fraction = if info.total_bytes == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss, reason = "a fraction for a progress bar")]
        {
            progressed_bytes as f32 / info.total_bytes as f32
        }
    };
    let down = rates.down;
    // The engine decides when it is done, not the ratio. Bytes can be all
    // present while pieces are still being verified, and a torrent resumed
    // over existing files counts what is on disk before it knows whether any
    // of it is wanted — either way the fraction reaches one while the engine
    // is still working, and a bar reading 100% beside the word Downloading is
    // the interface contradicting itself.
    let complete = if source_checking {
        fraction.min(0.9999)
    } else if info.finished {
        1.0
    } else {
        fraction.min(0.9999)
    };
    Some(Transfer {
        progress: complete.clamp(0.0, 1.0),
        source_reading: source_checking,
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
fn status_of_for(info: &TorrentInfo, paused: bool, local_source: bool) -> Status {
    if paused {
        return Status::Paused;
    }
    if local_source {
        return Status::Seeding;
    }
    crate::nexus::projection::state::status_for(crate::nexus::projection::state::StatusFacts {
        lifecycle: crate::nexus::store::records::StoredLifecycle::TorrentRequested {
            activity: crate::nexus::store::records::StoredActivity::Running,
        },
        carried: true,
        publishing: false,
        importing: false,
        locally_complete: local_source,
        live: Some(info),
    })
}

/// Native receive and upload activity since the last reading, per second.
///
/// Download activity is measured from librqbit's per-torrent `fetched_bytes`
/// counter rather than verified progress or its session-wide smoothed-speed
/// estimator. `fetched_bytes` advances as bytes arrive from peers, while
/// `progress_bytes` advances later when a full piece passes hash verification.
/// This preserves verified progress as the completion truth while showing a
/// receiver the actual per-torrent network activity that caused it.
///
/// The rate is still a delta over real elapsed time: when librqbit reports no
/// received bytes in an interval, this is exactly zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rates {
    pub down: u32,
    pub up: u32,
}

/// What was last written for one collection and the native counters it was
/// measured against — a rate is a difference, so it needs both ends.
#[derive(Clone)]
struct LastReading {
    sample: StoredSample,
    at: u64,
    fetched: u64,
    uploaded: u64,
    /// Per-peer counters at that moment, keyed by address, so a peer's rate
    /// can be measured the same way the collection's is. Peers that went away
    /// are simply absent next tick and measure as gone rather than as idle.
    peers: HashMap<String, (u64, u64)>,
}

/// Per-peer live rates for one reading.
///
/// Measured between polls rather than divided over the connection's lifetime.
/// A peer that delivered a burst and then went quiet reads as idle here, which
/// is what a person watching the transfer actually sees; a lifetime average
/// would keep claiming it was still working.
fn measured_peers(
    info: &TorrentInfo,
    last: Option<&LastReading>,
    elapsed_ns: u64,
) -> Vec<crate::nexus::projection::state::PeerState> {
    info.live_peer_addrs
        .iter()
        .map(|peer| {
            let previous = last
                .filter(|_| elapsed_ns > 0)
                .and_then(|reading| reading.peers.get(&peer.address));
            let (down, up) = previous.map_or((0, 0), |(fetched, uploaded)| {
                (
                    rate(peer.fetched_bytes.saturating_sub(*fetched), elapsed_ns),
                    rate(peer.uploaded_bytes.saturating_sub(*uploaded), elapsed_ns),
                )
            });
            crate::nexus::projection::state::PeerState {
                address: peer.address.clone(),
                client: peer.client.clone(),
                down_bytes: peer.fetched_bytes,
                up_bytes: peer.uploaded_bytes,
                down_bytes_per_second: down,
                up_bytes_per_second: up,
            }
        })
        .collect()
}

fn peer_counters(info: &TorrentInfo) -> HashMap<String, (u64, u64)> {
    info.live_peer_addrs
        .iter()
        .map(|peer| {
            (
                peer.address.clone(),
                (peer.fetched_bytes, peer.uploaded_bytes),
            )
        })
        .collect()
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
        down: rate(info.fetched_bytes.saturating_sub(previous.fetched), elapsed),
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
            source_check_bytes: None,
            fetched_bytes: 0,
            total_bytes: total,
            uploaded_bytes: 0,
            finished,
            error: None,
            files: Vec::new(),
            live_peers: 3,
            live_peer_addrs: vec![crate::nexus::torrent::PeerLink {
                address: "10.0.0.1:6881".to_owned(),
                fetched_bytes: 0,
                uploaded_bytes: 0,
                client: None,
            }],
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

    /// Receive activity must come from librqbit's native per-torrent counter,
    /// not verified-piece progress. A connected peer can be delivering chunks
    /// for a piece while verified progress stays unchanged until its hash
    /// passes, so a verified-byte delta would falsely present that active
    /// receive interval as zero.
    #[test]
    fn a_rate_uses_librqbit_native_received_byte_counter() {
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
            fetched: 0,
            uploaded: 0,
            peers: HashMap::new(),
        };

        let mut receiving = arrived;
        receiving.fetched_bytes = 2_000_000;
        let measured = measured_rates(&receiving, Some(&a_second_ago));
        assert!(
            (1_900_000..=2_100_000).contains(&measured.down),
            "about two native-received megabytes in about a second, got {}",
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
            lifecycle: crate::nexus::store::records::StoredLifecycle::TorrentRequested {
                activity: crate::nexus::store::records::StoredActivity::Running,
            },
            on_disk_bytes: 0,
            substrate_handle: Some("abc".to_owned()),
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
        let transfer = transfer_of_for(&every_byte, Rates::default(), false).expect("still moving");
        assert!(
            transfer.progress < 1.0,
            "unfinished cannot report complete, got {}",
            transfer.progress
        );

        // And once it does say so, nothing holds it back.
        let mut done = info(100, 100, true);
        done.live_peers = 1;
        assert_eq!(
            transfer_of_for(&done, Rates::default(), false)
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
        assert_eq!(transfer_of_for(&done, Rates::default(), false), None);

        // Serving somebody: measured upload, so it is genuinely not idle.
        let seeding = TorrentInfo {
            live_peers: 2,
            ..done.clone()
        };
        let uploading = Rates {
            down: 0,
            up: 500_000,
        };
        let seeding = transfer_of_for(&seeding, uploading, false).expect("still moving");
        assert!((seeding.progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(seeding.up_bytes_per_second, 500_000);
        assert_eq!(seeding.eta_secs, None, "nothing left to arrive");

        let downloading = Rates {
            down: 1_000_000,
            up: 0,
        };
        let moving =
            transfer_of_for(&info(50, 100, false), downloading, false).expect("still moving");
        assert!((moving.progress - 0.5).abs() < f32::EPSILON);
        assert_eq!(moving.peers, 3);
        assert_eq!(moving.eta_secs, Some(50 / 1_000_000));
        assert!(!moving.source_reading);
    }

    #[test]
    fn linked_source_verification_is_not_reported_as_a_download() {
        let mut checking = info(0, 547, false);
        checking.state = "Initializing".to_owned();
        checking.source_check_bytes = Some(61);
        let source_read = Rates {
            down: 6_300_000,
            up: 0,
        };

        let reading = transfer_of_for(&checking, source_read, true)
            .expect("owner source reads remain visible");
        assert!((reading.progress - (61.0 / 547.0)).abs() < f32::EPSILON);
        assert_eq!(reading.down_bytes_per_second, 6_300_000);
        assert!(reading.source_reading);
        assert_eq!(
            status_of_for(&checking, false, true),
            Status::Seeding,
            "source verification is seed preparation, never a receiver download"
        );

        let mut seeded = info(547, 547, true);
        seeded.live_peers = 1;
        let upload = transfer_of_for(
            &seeded,
            Rates {
                down: 0,
                up: 500_000,
            },
            true,
        )
        .expect("an owner serving a peer is active");
        assert_eq!(upload.down_bytes_per_second, 0);
        assert_eq!(upload.up_bytes_per_second, 500_000);
        assert_eq!(upload.progress, 1.0);
        assert!(
            !upload.source_reading,
            "serving a peer is upload activity, not source verification"
        );
    }

    /// A total of zero is a torrent whose metadata has not arrived. Dividing by
    /// it would be a progress bar full of NaN.
    #[test]
    fn a_transfer_with_no_known_total_reports_no_progress_rather_than_nan() {
        let unknown =
            transfer_of_for(&info(0, 0, false), Rates::default(), false).expect("carried");

        assert!((unknown.progress - 0.0).abs() < f32::EPSILON);
        assert_eq!(unknown.eta_secs, None, "nothing to estimate from");
    }

    /// The same rule as the projection builder: a person's choice outranks the
    /// numbers, and both places have to say so or one screen contradicts the
    /// other.
    #[test]
    fn a_pause_outranks_whatever_the_numbers_are_doing() {
        assert_eq!(
            status_of_for(&info(50, 100, false), true, false),
            Status::Paused
        );
        assert_eq!(
            status_of_for(&info(50, 100, false), false, false),
            Status::Downloading
        );
        assert_eq!(
            status_of_for(&info(0, 100, false), false, false),
            Status::Preparing
        );
        assert_eq!(
            status_of_for(&info(100, 100, true), false, false),
            Status::Available
        );
        assert_eq!(
            status_of_for(&info(100, 100, true), true, false),
            Status::Paused,
            "a finished collection a person paused is still paused"
        );
    }

    #[test]
    fn holdings_answer_with_the_last_reading_and_nothing_for_the_rest() {
        let holdings = Holdings::default();
        assert!(holdings.get(b"unknown").is_none());

        holdings.replace(HashMap::from([(
            b"key".to_vec(),
            Holding {
                info: info(1, 2, false),
                peers: Vec::new(),
            },
        )]));

        assert_eq!(
            holdings.get(b"key").map(|info| info.progress_bytes),
            Some(1)
        );
        assert!(holdings.get(b"other").is_none());
        assert!(
            holdings.peers(b"other").is_empty(),
            "a collection nothing is carrying has no peers rather than stale ones"
        );
    }

    /// A peer's rate is the difference between two polls, not its share of the
    /// connection's lifetime average. A peer that delivered a burst and went
    /// quiet reads as idle, which is what the person watching actually sees.
    #[test]
    fn a_peer_rate_is_measured_between_polls_rather_than_averaged() {
        let peer = |fetched: u64, uploaded: u64| crate::nexus::torrent::PeerLink {
            address: "10.0.0.7:6881".to_owned(),
            fetched_bytes: fetched,
            uploaded_bytes: uploaded,
            client: Some("qBittorrent 4.6".to_owned()),
        };
        let mut reading = info(0, 100, false);
        reading.live_peer_addrs = vec![peer(3_000_000, 1_000_000)];

        let a_second_ago = LastReading {
            sample: StoredSample {
                done: 0,
                total: 100,
                down_bytes_per_second: 0,
                up_bytes_per_second: 0,
                peers: 1,
            },
            at: unix_time_ns().saturating_sub(1_000_000_000),
            fetched: 0,
            uploaded: 0,
            peers: HashMap::from([("10.0.0.7:6881".to_owned(), (1_000_000, 500_000))]),
        };

        let measured = measured_peers(&reading, Some(&a_second_ago), 1_000_000_000);
        assert_eq!(measured.len(), 1);
        assert_eq!(measured[0].down_bytes_per_second, 2_000_000);
        assert_eq!(measured[0].up_bytes_per_second, 500_000);
        // The totals stay the connection's own counters, not the delta.
        assert_eq!(measured[0].down_bytes, 3_000_000);
        assert_eq!(measured[0].up_bytes, 1_000_000);
        assert_eq!(measured[0].client.as_deref(), Some("qBittorrent 4.6"));

        // A peer seen for the first time has nothing to measure against, and
        // reports no rate rather than its whole connection as one tick.
        let first_sight = measured_peers(&reading, None, 0);
        assert_eq!(first_sight[0].down_bytes_per_second, 0);
        assert_eq!(
            first_sight[0].down_bytes, 3_000_000,
            "but its totals are still true"
        );
    }
}
