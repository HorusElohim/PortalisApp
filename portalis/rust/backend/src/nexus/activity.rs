//! Backend-owned local device activity and bounded app-run accounting.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::nexus::store::{
    Store, StoreError,
    records::{AppRunEnd, StoredAppRun, StoredDeviceActivity},
};

/// A presentation-neutral snapshot of the durable ledger and bounded runs.
#[derive(Clone, Debug)]
pub struct DeviceActivitySnapshot {
    pub activity: StoredDeviceActivity,
    pub run: StoredAppRun,
    pub recent_runs: Vec<StoredAppRun>,
}

#[derive(Clone)]
pub struct DeviceActivityTracker {
    inner: Arc<TrackerInner>,
}

impl std::fmt::Debug for DeviceActivityTracker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceActivityTracker")
            .finish_non_exhaustive()
    }
}

struct TrackerInner {
    store: Arc<Store>,
    state: Mutex<TrackerState>,
}

struct TrackerState {
    activity: StoredDeviceActivity,
    run: StoredAppRun,
    counters: HashMap<Vec<u8>, (u64, u64)>,
    last_accounted_at: u64,
    active: bool,
}

impl TrackerState {
    fn advance_time(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.last_accounted_at);
        self.activity.engine_running_ns = self.activity.engine_running_ns.saturating_add(elapsed);
        self.run.engine_running_ns = self.run.engine_running_ns.saturating_add(elapsed);
        if self.active {
            self.activity.foreground_ns = self.activity.foreground_ns.saturating_add(elapsed);
            self.run.foreground_ns = self.run.foreground_ns.saturating_add(elapsed);
        }
        self.last_accounted_at = now.max(self.last_accounted_at);
    }
}

impl DeviceActivityTracker {
    /// Starts a new run and recovers a previously open run as interrupted.
    ///
    /// # Errors
    /// Returns [`StoreError`] when durable state cannot be read or checkpointed.
    pub fn start(store: Arc<Store>, now: u64) -> Result<Self, StoreError> {
        let mut activity = store.device_activity()?.unwrap_or(StoredDeviceActivity {
            stats_started_at: now,
            ..Default::default()
        });
        let mut recent = store.app_runs()?;
        if let Some(previous) = recent
            .first_mut()
            .filter(|run| run.end_reason == AppRunEnd::Current)
        {
            previous.ended_at = Some(now);
            previous.last_checkpoint_at = now;
            previous.end_reason = AppRunEnd::Interrupted;
            activity.runs_interrupted = activity.runs_interrupted.saturating_add(1);
            activity.last_activity_at = now;
            store.checkpoint_device_activity(&activity, previous)?;
        }

        let previous_id = recent.first().map_or(0, |run| run.run_id);
        let run_id = now.max(previous_id.saturating_add(1));
        activity.runs_started = activity.runs_started.saturating_add(1);
        activity.last_activity_at = now;
        let run = StoredAppRun {
            run_id,
            started_at: now,
            last_checkpoint_at: now,
            end_reason: AppRunEnd::Current,
            ..Default::default()
        };
        store.checkpoint_device_activity(&activity, &run)?;

        Ok(Self {
            inner: Arc::new(TrackerInner {
                store,
                state: Mutex::new(TrackerState {
                    activity,
                    run,
                    counters: HashMap::new(),
                    last_accounted_at: now,
                    active: true,
                }),
            }),
        })
    }

    /// Observes raw engine counters. Repeated values add zero; a decrease starts
    /// a new counter segment. Local-source reads are not network downloads.
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &self,
        key: &[u8],
        fetched_bytes: u64,
        uploaded_bytes: u64,
        down_bytes_per_second: u32,
        up_bytes_per_second: u32,
        local_source: bool,
        completed_download: bool,
        now: u64,
    ) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.advance_time(now);
        let previous = state
            .counters
            .insert(key.to_vec(), (fetched_bytes, uploaded_bytes));
        let (down_delta, up_delta) = previous.map_or((fetched_bytes, uploaded_bytes), |old| {
            (
                if fetched_bytes >= old.0 {
                    fetched_bytes - old.0
                } else {
                    fetched_bytes
                },
                if uploaded_bytes >= old.1 {
                    uploaded_bytes - old.1
                } else {
                    uploaded_bytes
                },
            )
        });
        let network_down = if local_source { 0 } else { down_delta };
        state.activity.total_network_down_bytes = state
            .activity
            .total_network_down_bytes
            .saturating_add(network_down);
        state.activity.total_network_up_bytes = state
            .activity
            .total_network_up_bytes
            .saturating_add(up_delta);
        state.run.network_down_bytes = state.run.network_down_bytes.saturating_add(network_down);
        state.run.network_up_bytes = state.run.network_up_bytes.saturating_add(up_delta);
        if completed_download && !local_source {
            state.activity.completed_downloads =
                state.activity.completed_downloads.saturating_add(1);
            state.run.completed_downloads = state.run.completed_downloads.saturating_add(1);
        }
        let network_down_rate = if local_source {
            0
        } else {
            down_bytes_per_second
        };
        state.activity.peak_down_bytes_per_second = state
            .activity
            .peak_down_bytes_per_second
            .max(network_down_rate);
        state.activity.peak_up_bytes_per_second = state
            .activity
            .peak_up_bytes_per_second
            .max(up_bytes_per_second);
        state.run.peak_down_bytes_per_second =
            state.run.peak_down_bytes_per_second.max(network_down_rate);
        state.run.peak_up_bytes_per_second =
            state.run.peak_up_bytes_per_second.max(up_bytes_per_second);
        if network_down > 0
            || up_delta > 0
            || completed_download
            || network_down_rate > 0
            || up_bytes_per_second > 0
        {
            state.activity.last_activity_at = now;
        }
    }

    /// Accounts a foreground/background transition and checkpoints backgrounding.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the background checkpoint fails.
    pub fn set_active(&self, active: bool, now: u64) -> Result<(), StoreError> {
        let should_checkpoint = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.advance_time(now);
            let changed = state.active != active;
            state.active = active;
            changed && !active
        };
        if should_checkpoint {
            self.checkpoint(now)?;
        }
        Ok(())
    }

    /// Persists an idempotent snapshot of cumulative and current-run values.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the transaction cannot be committed.
    pub fn checkpoint(&self, now: u64) -> Result<(), StoreError> {
        let (activity, run) = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.advance_time(now);
            state.run.last_checkpoint_at = now;
            (state.activity.clone(), state.run.clone())
        };
        self.inner.store.checkpoint_device_activity(&activity, &run)
    }

    /// Marks this run as a graceful close and persists it.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the transaction cannot be committed.
    pub fn finish(&self, now: u64) -> Result<(), StoreError> {
        let (activity, run) = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.advance_time(now);
            if state.run.end_reason == AppRunEnd::Current {
                state.run.end_reason = AppRunEnd::Graceful;
                state.run.ended_at = Some(now);
                state.activity.runs_completed_cleanly =
                    state.activity.runs_completed_cleanly.saturating_add(1);
                state.activity.last_clean_shutdown_at = now;
            }
            state.run.last_checkpoint_at = now;
            (state.activity.clone(), state.run.clone())
        };
        self.inner.store.checkpoint_device_activity(&activity, &run)
    }

    /// Returns current in-memory truth plus bounded durable runs.
    ///
    /// # Errors
    /// Returns [`StoreError`] when recent runs cannot be read.
    pub fn snapshot(&self) -> Result<DeviceActivitySnapshot, StoreError> {
        let (activity, run) = {
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (state.activity.clone(), state.run.clone())
        };
        let mut recent_runs = self.inner.store.app_runs()?;
        if let Some(current) = recent_runs
            .iter_mut()
            .find(|saved| saved.run_id == run.run_id)
        {
            *current = run.clone();
        } else {
            recent_runs.insert(0, run.clone());
        }
        Ok(DeviceActivitySnapshot {
            activity,
            run,
            recent_runs,
        })
    }

    /// Clears only activity history and starts a new tracking epoch/run.
    ///
    /// # Errors
    /// Returns [`StoreError`] when clearing or checkpointing fails.
    pub fn clear(&self, now: u64) -> Result<(), StoreError> {
        self.inner.store.clear_device_activity()?;
        let (activity, run) = {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let active = state.active;
            state.activity = StoredDeviceActivity {
                stats_started_at: now,
                runs_started: 1,
                last_activity_at: now,
                ..Default::default()
            };
            state.run = StoredAppRun {
                run_id: now,
                started_at: now,
                last_checkpoint_at: now,
                end_reason: AppRunEnd::Current,
                ..Default::default()
            };
            state.counters.clear();
            state.last_accounted_at = now;
            state.active = active;
            (state.activity.clone(), state.run.clone())
        };
        self.inner.store.checkpoint_device_activity(&activity, &run)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::nexus::store::{Store, records::AppRunEnd};

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("portalis-activity-{name}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("creates scratch directory");
            Self(path)
        }

        fn store(&self) -> Arc<Store> {
            Arc::new(Store::open(self.0.join("portalis.redb")).expect("opens store"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn network_deltas_are_idempotent_and_zero_copy_reads_are_not_downloads() {
        let scratch = Scratch::new("deltas");
        let tracker = DeviceActivityTracker::start(scratch.store(), 100).expect("starts");

        tracker.observe(b"received", 400, 100, 40, 10, false, false, 110);
        tracker.observe(b"received", 400, 100, 0, 0, false, false, 120);
        tracker.observe(b"received", 550, 160, 15, 6, false, true, 130);
        tracker.observe(b"local", 900, 70, 90, 7, true, false, 140);
        tracker.checkpoint(150).expect("checkpoints");

        let snapshot = tracker.snapshot().expect("reads snapshot");
        assert_eq!(snapshot.activity.total_network_down_bytes, 550);
        assert_eq!(snapshot.activity.total_network_up_bytes, 230);
        assert_eq!(snapshot.activity.completed_downloads, 1);
        assert_eq!(snapshot.activity.peak_down_bytes_per_second, 40);
        assert_eq!(snapshot.activity.peak_up_bytes_per_second, 10);
        assert_eq!(snapshot.run.network_down_bytes, 550);
        assert_eq!(snapshot.run.network_up_bytes, 230);
    }

    #[test]
    fn an_unfinished_run_is_recovered_as_interrupted() {
        let scratch = Scratch::new("recovery");
        let store = scratch.store();
        let first = DeviceActivityTracker::start(Arc::clone(&store), 100).expect("starts");
        first.observe(b"torrent", 50, 25, 5, 2, false, false, 120);
        first.checkpoint(130).expect("checkpoints");
        drop(first);

        let second = DeviceActivityTracker::start(store, 200).expect("recovers");
        let snapshot = second.snapshot().expect("reads snapshot");
        assert_eq!(snapshot.activity.runs_started, 2);
        assert_eq!(snapshot.activity.runs_interrupted, 1);
        assert_eq!(snapshot.recent_runs.len(), 2);
        assert_eq!(snapshot.recent_runs[1].end_reason, AppRunEnd::Interrupted);
        assert_eq!(snapshot.recent_runs[1].ended_at, Some(200));
        assert_eq!(snapshot.run.end_reason, AppRunEnd::Current);
    }
}
