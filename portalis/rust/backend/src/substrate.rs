//! What moves the bytes.
//!
//! The collection model says a claim points at content; it does not say
//! BitTorrent. Naming that boundary is what lets `fetch` and `delete` be
//! tested at all — they are the two paths where every remaining bug can hide,
//! and until now exercising either meant a real swarm.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::torrent::{PublishProgress, SourceFile, TorrentInfo};

/// The immutable content claim produced while a set of native sources starts
/// seeding. Keeping the descriptor with the engine result prevents collection
/// workflows from reaching into an engine-specific side store.
pub(crate) struct Published {
    pub(crate) info: TorrentInfo,
    pub(crate) descriptor: Vec<u8>,
}

/// What a source turns out to contain, before any payload is fetched.
///
/// The same answer for a `.torrent` file and for a magnet, which is the point:
/// a person chooses files the same way regardless of how they named the
/// content. The difference is only in how long the answer takes — a
/// descriptor is read from disk, a magnet is asked of the swarm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Inspected {
    pub(crate) info_hash: String,
    pub(crate) name: String,
    pub(crate) files: Vec<crate::torrent::TorrentMetadataFile>,
    /// The descriptor bytes, once known. A collection needs these to be
    /// shareable through Nexus later, and for a magnet they do not exist
    /// until the swarm supplies them.
    pub(crate) descriptor: Vec<u8>,
}

#[async_trait]
pub(crate) trait Substrate: Send + Sync {
    /// Make these files available, and answer with the handle others fetch by.
    async fn publish(
        &self,
        name: String,
        files: Vec<SourceFile>,
        progress: PublishProgress,
    ) -> anyhow::Result<Published>;

    /// Resolve what a `.torrent` path or magnet URI contains, fetching no
    /// payload.
    ///
    /// Separate from [`Self::acquire_selection`] because a person cannot
    /// choose files they have not been shown, and for a magnet that list is
    /// only knowable from the swarm. Bounded by the caller: a magnet whose
    /// swarm never answers must not hang a command forever.
    async fn inspect(&self, source: &str) -> anyhow::Result<Inspected>;

    /// Start fetching exactly `files` (indices into [`Inspected::files`])
    /// into `destination`.
    ///
    /// An empty selection is a caller error, not an instruction to fetch
    /// everything: "download nothing" and "download all of it" must never be
    /// the same request.
    async fn acquire_selection(
        &self,
        source: &str,
        files: &[usize],
        destination: &std::path::Path,
    ) -> anyhow::Result<TorrentInfo>;

    /// Stop or resume moving bytes for this handle.
    ///
    /// Idempotent: asking for a state the engine is already in is not an
    /// error, which is what lets the reconciler simply assert the stored
    /// intent on every pass rather than tracking what it last applied.
    async fn set_paused(&self, handle: &str, paused: bool) -> anyhow::Result<()>;

    /// Stop carrying it. Files already on disk stay there.
    async fn release(&self, handle: &str) -> anyhow::Result<()>;

    /// Everything held right now.
    async fn holdings(&self) -> Vec<TorrentInfo>;

}

/// The real one.
pub(crate) struct Torrents;

#[async_trait]
impl Substrate for Torrents {
    async fn publish(
        &self,
        name: String,
        files: Vec<SourceFile>,
        progress: PublishProgress,
    ) -> anyhow::Result<Published> {
        let info = crate::torrent::publish(name, files, progress).await?;
        let descriptor = crate::linked_source_store::descriptor_for(&info.info_hash)?;
        Ok(Published { info, descriptor })
    }

    async fn inspect(&self, source: &str) -> anyhow::Result<Inspected> {
        crate::torrent::inspect_source(source).await
    }

    async fn acquire_selection(
        &self,
        source: &str,
        files: &[usize],
        destination: &std::path::Path,
    ) -> anyhow::Result<TorrentInfo> {
        crate::torrent::acquire_selection(source, files, destination).await
    }

    async fn set_paused(&self, handle: &str, paused: bool) -> anyhow::Result<()> {
        if paused {
            crate::torrent::pause_torrent(handle).await
        } else {
            crate::torrent::restart_torrent(handle).await
        }
    }

    async fn release(&self, handle: &str) -> anyhow::Result<()> {
        crate::torrent::forget_torrent(handle).await
    }

    async fn holdings(&self) -> Vec<TorrentInfo> {
        crate::torrent::list_torrents().await.unwrap_or_default()
    }

}

static CURRENT: Mutex<Option<Arc<dyn Substrate>>> = Mutex::new(None);

pub(crate) fn current() -> Arc<dyn Substrate> {
    CURRENT
        .lock()
        .unwrap()
        .get_or_insert_with(|| Arc::new(Torrents))
        .clone()
}

/// Records what was asked of it and answers from a list. Enough to test the
/// two paths that decide *what* to move, which is where the bugs have been —
/// nothing here pretends to move anything.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Recorded {
    pub(crate) held: Mutex<Vec<String>>,
    pub(crate) released: Mutex<Vec<String>>,
    pub(crate) published: Mutex<Vec<String>>,
    /// Every `(source, files, destination)` a selection was started for.
    pub(crate) selections: Mutex<Vec<(String, Vec<usize>, std::path::PathBuf)>>,
    /// Every source `inspect` was asked about, in order.
    pub(crate) inspected: Mutex<Vec<String>>,
    /// Every `(handle, paused)` the engine was told to apply.
    pub(crate) paused: Mutex<Vec<(String, bool)>>,
    publication: Mutex<Option<(String, Vec<u8>)>>,
    inspection: Mutex<Option<Inspected>>,
}

#[cfg(test)]
impl Recorded {
    pub(crate) fn publishing(info_hash: String, descriptor: Vec<u8>) -> Self {
        Self {
            publication: Mutex::new(Some((info_hash, descriptor))),
            ..Self::default()
        }
    }

    /// A double that resolves any source to `inspection`. What a real magnet
    /// or descriptor contains is the engine's business; what Nexus does with
    /// the answer is what these tests are about.
    pub(crate) fn inspecting(inspection: Inspected) -> Self {
        Self {
            inspection: Mutex::new(Some(inspection)),
            ..Self::default()
        }
    }
}

#[cfg(test)]
#[async_trait]
impl Substrate for Recorded {
    async fn publish(
        &self,
        name: String,
        _files: Vec<SourceFile>,
        _progress: PublishProgress,
    ) -> anyhow::Result<Published> {
        self.published.lock().unwrap().push(name.clone());
        let publication = self.publication.lock().unwrap().clone();
        let Some((info_hash, descriptor)) = publication else {
            anyhow::bail!("publish is not configured for this double ({name})");
        };
        Ok(Published {
            info: held_torrent(&info_hash),
            descriptor,
        })
    }

    async fn inspect(&self, source: &str) -> anyhow::Result<Inspected> {
        self.inspected.lock().unwrap().push(source.to_string());
        let inspection = self.inspection.lock().unwrap().clone();
        inspection.ok_or_else(|| anyhow::anyhow!("inspect is not configured for this double"))
    }

    async fn acquire_selection(
        &self,
        source: &str,
        files: &[usize],
        destination: &std::path::Path,
    ) -> anyhow::Result<TorrentInfo> {
        self.selections.lock().unwrap().push((
            source.to_string(),
            files.to_vec(),
            destination.to_path_buf(),
        ));
        let inspection = self.inspection.lock().unwrap().clone();
        let Some(inspection) = inspection else {
            anyhow::bail!("acquire_selection is not configured for this double");
        };
        Ok(held_torrent(&inspection.info_hash))
    }

    async fn set_paused(&self, handle: &str, paused: bool) -> anyhow::Result<()> {
        self.paused
            .lock()
            .unwrap()
            .push((handle.to_string(), paused));
        Ok(())
    }

    async fn release(&self, handle: &str) -> anyhow::Result<()> {
        self.released.lock().unwrap().push(handle.to_string());
        Ok(())
    }

    async fn holdings(&self) -> Vec<TorrentInfo> {
        self.held
            .lock()
            .unwrap()
            .iter()
            .map(String::as_str)
            .map(held_torrent)
            .collect()
    }

}

#[cfg(test)]
fn held_torrent(info_hash: &str) -> TorrentInfo {
    TorrentInfo {
        id: 0,
        info_hash: info_hash.to_string(),
        name: "held".into(),
        state: "live".into(),
        progress_bytes: 1,
        total_bytes: 1,
        uploaded_bytes: 0,
        download_mbps: 0.0,
        upload_mbps: 0.0,
        finished: true,
        error: None,
        live_peers: 0,
        live_peer_addrs: Vec::new(),
        files: Vec::new(),
    }
}
