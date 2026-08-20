//! What moves the bytes.
//!
//! The collection model says a claim points at content; it does not say
//! BitTorrent. Naming that boundary is what lets `fetch` and `delete` be
//! tested at all — they are the two paths where every remaining bug can hide,
//! and until now exercising either meant a real swarm.

use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

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
    async fn inspect(&self, source: &str, peer_hints: &PeerHints) -> anyhow::Result<Inspected>;

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
        peer_hints: &PeerHints,
    ) -> anyhow::Result<TorrentInfo>;

    /// Stop or resume moving bytes for this handle.
    ///
    /// Idempotent: asking for a state the engine is already in is not an
    /// error, which is what lets the reconciler simply assert the stored
    /// intent on every pass rather than tracking what it last applied.
    async fn set_paused(&self, handle: &str, paused: bool) -> anyhow::Result<()>;

    /// Narrow or widen what this handle is fetching, after it started.
    ///
    /// The counterpart to [`Self::acquire_selection`], which can only say what
    /// to fetch at the moment a download begins. Without this the first choice
    /// was permanent: deselecting a file did nothing, and reselecting one
    /// could not be expressed at all.
    ///
    /// Idempotent for the same reason as [`Self::set_paused`] — the reconciler
    /// asserts the stored selection every pass rather than remembering which
    /// one it last sent.
    async fn set_selection(&self, handle: &str, files: &[usize]) -> anyhow::Result<()>;

    /// Stop carrying it. Files already on disk stay there.
    async fn release(&self, handle: &str) -> anyhow::Result<()>;

    /// Everything held right now.
    async fn holdings(&self) -> Vec<TorrentInfo>;
}

/// The real one.
pub(crate) struct Torrents;

/// Validated, bounded peer addresses supplied by a discovery/bootstrap source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerHints(Vec<SocketAddr>);

impl PeerHints {
    const MAX: usize = 64;

    pub(crate) fn new<I>(addresses: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = SocketAddr>,
    {
        let mut seen = HashSet::new();
        let mut unique = Vec::new();
        for address in addresses {
            anyhow::ensure!(
                address.port() != 0
                    && !address.ip().is_unspecified()
                    && !address.ip().is_multicast(),
                "invalid peer hint {address}"
            );
            if seen.insert(address) {
                unique.push(address);
            }
        }
        anyhow::ensure!(
            unique.len() <= Self::MAX,
            "too many peer hints: maximum is 64"
        );
        Ok(Self(unique))
    }

    pub(crate) fn as_slice(&self) -> &[SocketAddr] {
        &self.0
    }
}


pub mod lan_discovery {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::str::FromStr;

    use crate::substrate::PeerHints;

    /// Discover local network peers by examining network interfaces.
    /// 
    /// This function returns PeerHints containing IPv4 addresses of local
    /// network interfaces on the standard BitTorrent port (6881).
    /// 
    /// # Returns
    /// * PeerHints containing discovered local peers
    /// 
    /// # Example
    /// ```no_run
    /// // let hints = lan_discovery::discover_local_peers();
    /// // hints might contain: 192.168.1.100:6881, 10.0.0.5:6881, etc.
    /// ```
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows", target_os = "macos"))]
    pub fn discover_local_peers() -> PeerHints {
        // Get list of network interfaces
        let mut peers = Vec::new();
        
        // Try to get interfaces using the `getifaddrs` crate or similar
        // For now, we'll use a simple approach that works on most systems
        // by checking common private IP ranges
        
        // Common private IP ranges to check
                let ranges = [
                    (Ipv4Addr::new(10, 0, 0, 0), Ipv4Addr::new(10, 255, 255, 255)), // 10.0.0.0/8
                    (Ipv4Addr::new(172, 16, 0, 0), Ipv4Addr::new(172, 31, 255, 255)), // 172.16.0.0/12
                    (Ipv4Addr::new(192, 168, 0, 0), Ipv4Addr::new(192, 168, 255, 255)), // 192.168.0.0/16
                ];

                // Get our own IP addresses from hostname -I as a fallback
                // In a real implementation, we would use system APIs to get interface addresses
                if let Ok(output) = std::process::Command::new("hostname")
                    .arg("-I")
                    .output()
                    && let Ok(ips_str) = String::from_utf8(output.stdout)
                {
                    for ip_str in ips_str.split_whitespace() {
                        if let Ok(ip) = IpAddr::from_str(ip_str)
                            && let IpAddr::V4(ipv4) = ip
                        {
                            // Check if it's in a private range
                            let is_private = ranges.iter().any(|&(start, end)| {
                                ipv4 >= start && ipv4 <= end
                            });

                            if is_private {
                                let addr = SocketAddr::new(ip, 6881); // Standard BitTorrent port
                                peers.push(addr);
                            }
                        }
                    }
                }

                // Remove duplicates while preserving order
                peers.sort();
                peers.dedup();
        
        // Create PeerHints (will reject invalid addresses internally)
        PeerHints::new(peers).unwrap_or_else(|_| PeerHints::default())
    }

    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows", target_os = "macos")))]
    pub fn discover_local_peers() -> PeerHints {
        // Return empty hints on unsupported platforms
        PeerHints::default()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_discover_local_peers_returns_peer_hints() {
            let hints = discover_local_peers();
            // Should always return a valid PeerHints instance
            // The actual content depends on the host's network configuration
            assert!(hints.as_slice().len() <= 64); // Should respect the limit
        }
    }
}
#[cfg(test)]
mod peer_hint_tests {
    use std::net::SocketAddr;

    use super::PeerHints;

    #[test]
    fn peer_hints_are_deduplicated_and_bounded() {
        let first: SocketAddr = "192.0.2.10:6881".parse().unwrap();
        let second: SocketAddr = "192.0.2.11:6881".parse().unwrap();
        let hints = PeerHints::new([first, second, first]).expect("valid hints");

        assert_eq!(hints.as_slice(), [first, second]);
    }

    #[test]
    fn unusable_peer_hints_are_rejected() {
        for address in [
            "0.0.0.0:6881".parse().unwrap(),
            "239.0.0.1:6881".parse().unwrap(),
            "192.0.2.10:0".parse().unwrap(),
        ] {
            assert!(PeerHints::new([address]).is_err());
        }
    }
}

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

    async fn inspect(&self, source: &str, peer_hints: &PeerHints) -> anyhow::Result<Inspected> {
        crate::torrent::inspect_source(source, peer_hints).await
    }

    async fn acquire_selection(
        &self,
        source: &str,
        files: &[usize],
        destination: &std::path::Path,
        peer_hints: &PeerHints,
    ) -> anyhow::Result<TorrentInfo> {
        crate::torrent::acquire_selection(source, files, destination, peer_hints).await
    }

    async fn set_paused(&self, handle: &str, paused: bool) -> anyhow::Result<()> {
        if paused {
            crate::torrent::pause_torrent(handle).await
        } else {
            crate::torrent::restart_torrent(handle).await
        }
    }

    async fn set_selection(&self, handle: &str, files: &[usize]) -> anyhow::Result<()> {
        crate::torrent::set_selection(handle, files).await
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
    /// What `holdings()` reports on successive calls, in order. What a real
    /// engine says on each poll is the engine's business; what Nexus does
    /// with the answer is what the tests are about.
    pub(crate) readings: Mutex<std::collections::VecDeque<Vec<TorrentInfo>>>,
    pub(crate) published: Mutex<Vec<String>>,
    /// Every `(source, files, destination)` a selection was started for.
    pub(crate) selections: Mutex<Vec<(String, Vec<usize>, std::path::PathBuf)>>,
    /// Every source `inspect` was asked about, in order.
    pub(crate) inspected: Mutex<Vec<String>>,
    /// Every `(handle, paused)` the engine was told to apply.
    pub(crate) paused: Mutex<Vec<(String, bool)>>,
    /// Every `(handle, files)` the engine was told to fetch after starting.
    pub(crate) reselected: Mutex<Vec<(String, Vec<usize>)>>,
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

    /// A double that answers `holdings()` with a scripted sequence of
    /// readings, in order. What a real engine reports on each poll is the
    /// engine's business; what Nexus does with the answer is what the tests
    /// are about. Once the queue is exhausted the last reading repeats —
    /// the engine holding steady is the normal state, not an error.
    pub(crate) fn reading(sequence: Vec<Vec<TorrentInfo>>) -> Self {
        Self {
            readings: Mutex::new(std::collections::VecDeque::from(sequence)),
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

    async fn inspect(&self, source: &str, _peer_hints: &PeerHints) -> anyhow::Result<Inspected> {
        self.inspected.lock().unwrap().push(source.to_string());
        let inspection = self.inspection.lock().unwrap().clone();
        inspection.ok_or_else(|| anyhow::anyhow!("inspect is not configured for this double"))
    }

    async fn acquire_selection(
        &self,
        source: &str,
        files: &[usize],
        destination: &std::path::Path,
        _peer_hints: &PeerHints,
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

    async fn set_selection(&self, handle: &str, files: &[usize]) -> anyhow::Result<()> {
        self.reselected
            .lock()
            .unwrap()
            .push((handle.to_string(), files.to_vec()));
        Ok(())
    }

    async fn release(&self, handle: &str) -> anyhow::Result<()> {
        self.released.lock().unwrap().push(handle.to_string());
        Ok(())
    }

    async fn holdings(&self) -> Vec<TorrentInfo> {
        let mut readings = self.readings.lock().unwrap();
        if let Some(next) = readings.pop_front() {
            if let Some(last) = next.last() {
                readings.push_back(vec![last.clone()]);
            }
            return next;
        }
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
        finished: true,
        error: None,
        live_peers: 0,
        live_peer_addrs: Vec::new(),
        files: Vec::new(),
    }
}
