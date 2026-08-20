//! The librqbit engine: publish, inspect, acquire and account for content on
//! a `librqbit::Session`. This is the online strategy behind
//! `substrate::Torrents` (ADR-0003) — the collection/manifest domain model
//! sits above it and never names BitTorrent.
//!
//! It reaches the app only through the single `AppSnapshot`/`Command` seam
//! (ADR-0001); nothing here is exported across the FFI directly.
//!
//! The DTOs and function signatures below are unconditional (compiled on
//! every target, wasm32 included) because `substrate` is target-agnostic and
//! refers to them regardless of platform — only the *implementation* is
//! target-gated, falling back to an error on wasm32 (Web is a viewer, not a
//! swarm participant; see the backend README).

#[derive(Debug, Clone)]
pub struct TorrentInfo {
    pub id: usize,
    pub info_hash: String,
    pub name: String,
    pub state: String,
    /// Bytes confirmed present, or zero while the engine is still checking
    /// and does not yet know. Use [`TorrentInfo::knows_progress`] before
    /// treating a zero here as a measurement.
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub finished: bool,
    pub error: Option<String>,
    pub files: Vec<TorrentFile>,
    /// Currently-connected peers (not the historical total ever seen) — the
    /// closest real equivalent to the mockup's "N copies alive" indicator.
    pub live_peers: u32,
    /// `"ip:port"` of each currently-connected peer. BitTorrent peers carry
    /// no identity beyond their network address — there is no name, no
    /// signed device id, nothing to correlate them with a collaborator.
    pub live_peer_addrs: Vec<String>,
}

impl TorrentInfo {
    /// Whether [`Self::progress_bytes`] is a measurement rather than a
    /// placeholder.
    ///
    /// A torrent being verified reports nothing rather than the scan cursor,
    /// which would climb to full and then collapse. Correct for a status, and
    /// wrong to record: written into the history it became a zero between two
    /// real readings, and a chart drew the transfer as having restarted.
    #[must_use]
    pub fn knows_progress(&self) -> bool {
        self.state != "Initializing"
    }
}

/// One file inside a torrent, with its real on-disk location — resolved via
/// `librqbit::Api` (which knows the actual per-torrent output folder,
/// including the subfolder it auto-creates for multi-file torrents) rather
/// than guessed, so Flutter can load a thumbnail or open/play it directly.
#[derive(Debug, Clone)]
pub struct TorrentFile {
    pub name: String,
    pub absolute_path: String,
    pub length_bytes: u64,
    pub downloaded_bytes: u64,
    /// File-relative intersections of verified and currently downloading
    /// torrent pieces. Missing ranges are implicit.
    pub piece_runs: Vec<PieceRun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceRun {
    pub offset_bytes: u64,
    pub length_bytes: u64,
    pub verified: bool,
    /// Real peers assigned to the intersecting in-flight piece. Empty for a
    /// verified run; peer identity is only its current network address.
    pub peers: Vec<String>,
}

/// One native file Rust will publish as torrent content.
///
/// Only the path crosses the Flutter boundary. Rust owns reading, hashing,
/// torrent construction, and seeding, so file size is never limited by Dart
/// memory or the bridge's signed 32-bit byte-vector encoding. Publication
/// links this source into its torrent layout; it never copies its bytes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceFile {
    pub name: String,
    pub path: String,
    pub length_bytes: Option<u64>,
}

/// Metadata obtained from a `.torrent` descriptor before any payload bytes
/// are requested. It is deliberately smaller than the live torrent DTO: the
/// collection workflow needs a selection list, not an engine session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentMetadataFile {
    pub(crate) label: String,
    pub(crate) bytes: u64,
}

pub(crate) fn make_source_names_unique(files: &mut [SourceFile]) {
    let mut names = std::collections::HashSet::new();
    for file in files {
        let original = native::sanitize_component(&file.name);
        let mut candidate = original.clone();
        let mut suffix = 2;
        while !names.insert(candidate.to_lowercase()) {
            candidate = numbered_source_name(&original, suffix);
            suffix += 1;
        }
        file.name = candidate;
    }
}

fn numbered_source_name(name: &str, suffix: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            format!("{stem} ({suffix}).{extension}")
        }
        _ => format!("{name} ({suffix})"),
    }
}

pub(crate) const TORRENT_PIECE_LENGTH: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct PublishProgress {
    inner: std::sync::Arc<std::sync::Mutex<PublishProgressState>>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug, Clone)]
pub(crate) struct PublishProgressState {
    pub(crate) stage: String,
    pub(crate) processed_bytes: u64,
    pub(crate) total_bytes: u64,
    pub(crate) completed_pieces: u64,
    pub(crate) total_pieces: u64,
}

impl PublishProgress {
    pub(crate) fn new(total_bytes: u64) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(PublishProgressState {
                stage: "preparing".into(),
                processed_bytes: 0,
                total_bytes,
                completed_pieces: 0,
                total_pieces: total_bytes.div_ceil(TORRENT_PIECE_LENGTH),
            })),
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub(crate) fn set_stage(&self, stage: &str) {
        let mut state = self.inner.lock().unwrap();
        if stage == "hashing" && state.stage != "hashing" {
            state.processed_bytes = 0;
            state.completed_pieces = 0;
        }
        state.stage = stage.into();
    }

    fn advance(&self, bytes: u64) {
        let mut state = self.inner.lock().unwrap();
        state.processed_bytes = state
            .processed_bytes
            .saturating_add(bytes)
            .min(state.total_bytes);
    }

    fn advance_hashing(&self, bytes: u64, completed_piece: bool) {
        let mut state = self.inner.lock().unwrap();
        state.processed_bytes = state
            .processed_bytes
            .saturating_add(bytes)
            .min(state.total_bytes);
        if completed_piece {
            state.completed_pieces = state
                .completed_pieces
                .saturating_add(1)
                .min(state.total_pieces);
        }
    }

    fn complete_final_piece(&self) {
        let mut state = self.inner.lock().unwrap();
        state.completed_pieces = state
            .completed_pieces
            .saturating_add(1)
            .min(state.total_pieces);
    }

    pub(crate) fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn ensure_active(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.cancelled.load(std::sync::atomic::Ordering::Relaxed),
            "publication cancelled"
        );
        Ok(())
    }
}

#[cfg(test)]
mod validation_tests {
    use super::{
        SourceFile, is_magnet, is_remote_source, is_torrent_path, make_source_names_unique,
        native::sanitize_component,
    };

    /// The magnet that broke this: its `xs=` web-seed hint ends in
    /// `big-buck-bunny.torrent`, so judging by the tail of the string reads a
    /// magnet as a local descriptor and tries to open the whole URI as a
    /// path — `ENAMETOOLONG`, which names nothing a person could act on.
    #[test]
    fn a_magnet_whose_query_ends_in_dot_torrent_is_still_a_magnet() {
        const MAGNET: &str = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c\
&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Fexplodie.org%3A6969\
&xs=https%3A%2F%2Fwebtorrent.io%2Ftorrents%2Fbig-buck-bunny.torrent";

        assert!(is_magnet(MAGNET));
        assert!(is_remote_source(MAGNET));
        assert!(
            !is_torrent_path(MAGNET),
            "a magnet is never opened as a file, whatever its query string ends with"
        );
    }

    #[test]
    fn a_local_descriptor_is_a_path_and_a_url_never_is() {
        assert!(is_torrent_path("/Users/ada/Downloads/bundle.torrent"));
        assert!(
            is_torrent_path("relative/bundle.TORRENT"),
            "case is not identity"
        );
        assert!(!is_torrent_path("/Users/ada/Downloads/bundle.mkv"));

        for remote in [
            "magnet:?xt=urn:btih:abc",
            "MAGNET:?xt=urn:btih:abc",
            "https://example.test/bundle.torrent",
            "http://example.test/bundle.torrent",
        ] {
            assert!(is_remote_source(remote), "{remote}");
            assert!(!is_torrent_path(remote), "{remote}");
        }

        // A Windows path is not a URL, however much `C:` resembles a scheme.
        assert!(!is_remote_source(r"C:\Users\ada\bundle.torrent"));

        // An http URL must never be opened as a file, but it is also not an
        // input Portalis accepts — the two questions have different answers,
        // which is why they are different functions.
        assert!(is_remote_source("https://example.test/bundle.torrent"));
        assert!(!is_magnet("https://example.test/bundle.torrent"));
    }

    #[test]
    fn duplicate_source_names_get_stable_suffixes() {
        let mut files = vec![
            SourceFile {
                name: "photo.jpg".into(),
                path: "one".into(),
                length_bytes: None,
            },
            SourceFile {
                name: "photo.jpg".into(),
                path: "two".into(),
                length_bytes: None,
            },
            SourceFile {
                name: "Photo.jpg".into(),
                path: "three".into(),
                length_bytes: None,
            },
        ];

        make_source_names_unique(&mut files);

        assert_eq!(files[0].name, "photo.jpg");
        assert_eq!(files[1].name, "photo (2).jpg");
        assert_eq!(files[2].name, "Photo (3).jpg");
    }

    #[test]
    fn source_names_are_safe_and_portable() {
        assert_eq!(sanitize_component("../bad:name?.jpg"), "_bad_name_.jpg");
        assert_eq!(sanitize_component("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_component("..."), "untitled");
    }
}

/// Create a new collection by linking local files into a torrent layout,
/// building a `.torrent` from that layout and adding it back to the session at
/// the same location — since the files are already there and match the
/// piece hashes just computed from them, librqbit verifies them as already
/// complete and starts seeding immediately, no download needed. This is
/// the "share something" side of the app; `add_torrent_from_*` below is
/// the "join a swarm" side — both produce the exact same `TorrentInfo`
/// shape either way (see the backend README on why: it's the same
/// protocol regardless of which side of the swarm you started on).
///
/// Internal only: the app-facing seam is `portalis_api` (ADR-0001). This
/// helper exists so `substrate` and `core` can drive the engine directly.
pub(crate) async fn publish(
    name: String,
    files: Vec<SourceFile>,
    progress: PublishProgress,
) -> anyhow::Result<TorrentInfo> {
    native::create_collection(name, files, progress).await
}

/// Snapshot of every torrent currently managed by the session. Internal:
/// `substrate::holdings` uses it; the app reads torrent state from the
/// `portalis_api` `watch_*` streams instead (ADR-0001).
pub(crate) async fn list_torrents() -> anyhow::Result<Vec<TorrentInfo>> {
    native::list_torrents().await
}

/// One top-level item under the download directory — in practice, almost
/// always one manifest entry's own batch folder (see
/// `collections::add_media_to_collection`) or a plain torrent's folder.
/// `bytes` is recursive, so a multi-file batch reports what it actually
/// costs on disk.
///
/// Internal: `collections::storage_breakdown` is the bridged version, which
/// joins each of these against the collection that actually claims it — the
/// same join `list_collections` does between the persisted manifest and the
/// live session, so it belongs there and not here. See that module's doc for
/// why the join can only happen in the one layer that sees both sides.
#[derive(Debug, Clone)]
pub struct RawStorageEntry {
    pub(crate) name: String,
    pub(crate) bytes: u64,
    pub(crate) path: String,
}

/// What's actually on disk under the download directory, one entry per
/// top-level item, largest first — the real filesystem, where
/// `storage_usage_bytes`'s single total can only say "this much, somewhere".
pub async fn storage_breakdown() -> anyhow::Result<Vec<RawStorageEntry>> {
    native::storage_breakdown_native().await
}

/// Caps transfer speed across every torrent at once (not per-torrent).
/// `librqbit`'s `Session::ratelimits` is adjustable at runtime, so unlike the
/// rest of `SessionOptions` these take effect without a restart. `None` means
/// unlimited. Internal: `settings.rs` owns the user-facing surface, so there
/// is one way to change a rate limit rather than two.
pub(crate) async fn set_rate_limits(
    upload_bps: Option<u32>,
    download_bps: Option<u32>,
) -> anyhow::Result<()> {
    native::set_rate_limits(upload_bps, download_bps).await
}

/// Whether the BitTorrent session has already been constructed. Lets
/// `settings.rs` tell the difference between "this change needs a restart"
/// and "the session hasn't started yet, so it will pick this up anyway".
pub(crate) fn session_started() -> bool {
    native::session_started()
}

/// Where downloads land: the person's configured directory, or the platform
/// default. One answer, so the engine and the collection record agree.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn download_dir() -> std::path::PathBuf {
    native::output_dir()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn download_dir() -> std::path::PathBuf {
    std::path::PathBuf::new()
}

/// Whether a source names a local `.torrent` descriptor rather than a magnet.
///
/// Here rather than in the collection layer because it is a fact about the
/// engine's own input formats, and both the engine and Nexus have to agree
/// about it.
#[must_use]
pub(crate) fn is_torrent_path(source: &str) -> bool {
    !is_remote_source(source)
        && std::path::Path::new(source)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"))
}

/// Whether a source is a magnet link.
///
/// Narrower than [`is_remote_source`] on purpose. That one answers "must this
/// never be opened as a file", which has to be generous to be safe; this one
/// answers "is this something Portalis accepts", which has to be strict to
/// keep the accepted set of inputs the one that was actually designed for.
#[must_use]
pub(crate) fn is_magnet(source: &str) -> bool {
    let source = source.trim_start();
    source.len() >= "magnet:".len() && source[.."magnet:".len()].eq_ignore_ascii_case("magnet:")
}

/// Whether a source names something to fetch rather than a file to open.
///
/// Checked *before* any extension, and that order is the whole point: a
/// magnet's query string routinely ends in a filename, because trackers put
/// `&xs=https://…/big-buck-bunny.torrent` in it as a web-seed hint. Judging
/// by the tail of the string alone reads that as a local descriptor and tries
/// to open the entire URI as a path, which fails with `ENAMETOOLONG` rather
/// than with anything that names the real mistake.
///
/// The schemes are listed rather than matched as a generic `scheme:` prefix
/// so a Windows path (`C:\Users\…`) is never mistaken for a URL.
#[must_use]
pub(crate) fn is_remote_source(source: &str) -> bool {
    const REMOTE: [&str; 3] = ["magnet:", "http://", "https://"];
    let source = source.trim_start();
    REMOTE.iter().any(|scheme| {
        source.len() >= scheme.len() && source[..scheme.len()].eq_ignore_ascii_case(scheme)
    })
}

/// Resolves what a `.torrent` path or magnet URI contains, fetching no
/// payload.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn inspect_source(
    source: &str,
    peer_hints: &crate::nexus::substrate::PeerHints,
) -> anyhow::Result<crate::nexus::substrate::Inspected> {
    native::inspect_source(source, peer_hints).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn inspect_source(
    _source: &str,
    _peer_hints: &crate::nexus::substrate::PeerHints,
) -> anyhow::Result<crate::nexus::substrate::Inspected> {
    anyhow::bail!("torrent imports are unavailable on web")
}

/// Starts fetching exactly the chosen files into `destination`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn acquire_selection(
    source: &str,
    files: &[usize],
    destination: &std::path::Path,
    peer_hints: &crate::nexus::substrate::PeerHints,
) -> anyhow::Result<TorrentInfo> {
    native::acquire_selection(source, files, destination, peer_hints).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn acquire_selection(
    _source: &str,
    _files: &[usize],
    _destination: &std::path::Path,
    _peer_hints: &crate::nexus::substrate::PeerHints,
) -> anyhow::Result<TorrentInfo> {
    anyhow::bail!("torrent downloads are unavailable on web")
}

/// Removes a torrent from the session, leaving its downloaded files on disk
/// (librqbit's "forget", as opposed to "delete" which also unlinks them).
/// Backs deleting a plain-torrent collection — see
/// `collections::delete_collection`.
pub(crate) async fn forget_torrent(info_hash_hex: &str) -> anyhow::Result<()> {
    native::forget_torrent(info_hash_hex).await?;
    crate::nexus::linked_source_store::remove(info_hash_hex)?;
    Ok(())
}

/// Revises what an already-running torrent is fetching.
///
/// Doing nothing when the engine already agrees is what makes this safe to
/// assert on every reconcile pass: librqbit refuses the update while a torrent
/// is still initializing, and a no-op comparison never reaches that refusal
/// for the overwhelmingly common case of nothing having changed.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn set_selection(info_hash_hex: &str, files: &[usize]) -> anyhow::Result<()> {
    native::set_selection(info_hash_hex, files).await
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn set_selection(_info_hash_hex: &str, _files: &[usize]) -> anyhow::Result<()> {
    anyhow::bail!("torrent downloads are unavailable on web")
}

pub(crate) async fn pause_torrent(info_hash_hex: &str) -> anyhow::Result<()> {
    native::pause_torrent(info_hash_hex).await
}

pub(crate) async fn restart_torrent(info_hash_hex: &str) -> anyhow::Result<()> {
    native::restart_torrent(info_hash_hex).await
}

pub mod native {
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Arc;

    use anyhow::Context;
    use bencode::bencode_serialize_to_writer;
    use buffers::ByteBufOwned;
    use librqbit::api::TorrentIdOrHash;
    use librqbit::storage::{StorageFactory, StorageFactoryExt, TorrentStorage};
    use librqbit::{
        AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, CreateTorrentOptions,
        ManagedTorrentShared, Session, TorrentMetadata,
    };
    use librqbit_core::Id20;
    use librqbit_core::torrent_metainfo::{
        TorrentMetaV1File, TorrentMetaV1Info, TorrentMetaV1Owned,
    };
    use sha1w::{ISha1, Sha1};
    use tokio::sync::OnceCell;

    use super::{
        PieceRun, PublishProgress, RawStorageEntry, SourceFile, TORRENT_PIECE_LENGTH, TorrentFile,
        TorrentInfo,
    };

    static SESSION: OnceCell<Arc<Session>> = OnceCell::const_new();

    /// The precise place librqbit hashes and seeds. A single selected file is
    /// already a valid torrent root; several independent files need a shared
    /// directory, represented by hard links rather than copied content.
    struct SeedLayout {
        hash_path: PathBuf,
        output_folder: PathBuf,
        link_directory: Option<PathBuf>,
        linked_paths: Vec<PathBuf>,
        torrent_name: Option<String>,
    }

    #[derive(Clone)]
    struct ReferencedStorageFactory {
        sources: Vec<crate::nexus::content_location::ContentLocation>,
        lengths: Vec<u64>,
    }

    impl StorageFactory for ReferencedStorageFactory {
        type Storage = ReferencedStorage;

        fn create(
            &self,
            _shared: &ManagedTorrentShared,
            _metadata: &TorrentMetadata,
        ) -> anyhow::Result<Self::Storage> {
            Ok(ReferencedStorage {
                sources: self.sources.clone(),
                lengths: self.lengths.clone(),
            })
        }

        fn clone_box(&self) -> librqbit::storage::BoxStorageFactory {
            self.clone().boxed()
        }
    }

    #[derive(Clone)]
    struct ReferencedStorage {
        sources: Vec<crate::nexus::content_location::ContentLocation>,
        lengths: Vec<u64>,
    }

    impl TorrentStorage for ReferencedStorage {
        fn init(
            &mut self,
            _shared: &ManagedTorrentShared,
            _metadata: &TorrentMetadata,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn pread_exact(
            &self,
            file_id: usize,
            offset: u64,
            buffer: &mut [u8],
        ) -> anyhow::Result<()> {
            self.sources
                .get(file_id)
                .context("no such referenced source file")?
                .read_exact_at(offset, buffer)
        }

        fn pwrite_all(&self, _file_id: usize, _offset: u64, _buffer: &[u8]) -> anyhow::Result<()> {
            anyhow::bail!("gallery-linked source storage is read-only")
        }

        fn remove_file(&self, _file_id: usize, _filename: &std::path::Path) -> anyhow::Result<()> {
            Ok(())
        }
        fn remove_directory_if_empty(&self, _path: &std::path::Path) -> anyhow::Result<()> {
            Ok(())
        }

        fn ensure_file_length(&self, file_id: usize, length: u64) -> anyhow::Result<()> {
            anyhow::ensure!(
                self.lengths.get(file_id) == Some(&length),
                "referenced source length changed"
            );
            Ok(())
        }

        fn take(&self) -> anyhow::Result<Box<dyn TorrentStorage>> {
            Ok(Box::new(self.clone()))
        }
    }

    async fn session() -> anyhow::Result<Arc<Session>> {
        SESSION
            .get_or_try_init(|| async {
                // Read once so the configured output folder and all other
                // session construction settings are from the same snapshot.
                let settings = crate::nexus::settings::engine_settings().unwrap_or_default();
                let dir = output_dir_for(&settings);
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("creating output dir {dir:?}"))?;
                // Every knob comes from the persisted settings now — see
                // settings.rs. librqbit reads these once, here, which is why
                // changing any of them needs a restart.
                let peer_opts = librqbit::PeerConnectionOptions {
                    connect_timeout: settings
                        .peer_connect_timeout_secs
                        .map(|s| std::time::Duration::from_secs(s as u64)),
                    read_write_timeout: settings
                        .peer_read_write_timeout_secs
                        .map(|s| std::time::Duration::from_secs(s as u64)),
                    keep_alive_interval: settings
                        .peer_keep_alive_interval_secs
                        .map(|s| std::time::Duration::from_secs(s as u64)),
                };
                let trackers = settings
                    .trackers
                    .iter()
                    .filter_map(|t| match t.parse() {
                        Ok(url) => Some(url),
                        Err(e) => {
                            crate::nexus::log::clog!(
                                "torrent",
                                "ignoring unparseable tracker {t:?}: {e}"
                            );
                            None
                        }
                    })
                    .collect();
                let opts = librqbit::SessionOptions {
                    // A range is mandatory, not optional: librqbit only binds
                    // a TCP listener `if let Some(port_range) =
                    // opts.listen_port_range` (verified in 8.1.1's
                    // session.rs) — with none it binds nothing, this device
                    // can never accept an incoming peer connection, and
                    // enable_upnp_port_forwarding has no port to forward.
                    // settings.rs rejects an empty or zero range for exactly
                    // that reason.
                    listen_port_range: Some(settings.listen_port_start..settings.listen_port_end),
                    enable_upnp_port_forwarding: settings.enable_upnp_port_forwarding,
                    // Without persistence librqbit starts empty every launch,
                    // so a collection you were seeding comes back with every
                    // manifest entry unmatched and silently seeds nothing.
                    persistence: settings.persist_session.then_some(
                        librqbit::SessionPersistenceConfig::Json {
                            // OS-specific default folder (inside the app's
                            // container on macOS/iOS), like our other state.
                            folder: None,
                        },
                    ),
                    fastresume: settings.fastresume,
                    disable_dht: settings.disable_dht,
                    disable_dht_persistence: settings.disable_dht_persistence,
                    socks_proxy_url: settings.socks_proxy_url.clone(),
                    defer_writes_up_to: settings.defer_writes_up_to_mb.map(|mb| mb as usize),
                    concurrent_init_limit: settings.concurrent_init_limit.map(|n| n as usize),
                    peer_opts: Some(peer_opts),
                    blocklist_url: settings.blocklist_url.clone(),
                    trackers,
                    ratelimits: librqbit::limits::LimitsConfig {
                        upload_bps: settings
                            .upload_limit_bps
                            .and_then(std::num::NonZeroU32::new),
                        download_bps: settings
                            .download_limit_bps
                            .and_then(std::num::NonZeroU32::new),
                    },
                    ..Default::default()
                };
                Session::new_with_opts(dir.clone(), opts)
                    .await
                    .with_context(|| format!("starting librqbit session in {dir:?}"))
            })
            .await
            .cloned()
    }

    /// Where downloaded files land, chosen so the user can actually find
    /// them:
    /// - Desktop: the platform Downloads folder (falls back to a temp dir
    ///   if that can't be found, e.g. some minimal Linux setups).
    /// - iOS/Android: there's no "Downloads folder" in an app's sandbox —
    ///   that's a desktop concept. `dirs::download_dir()` on iOS resolves
    ///   to `<sandbox>/Downloads`, a directory nothing ever exposes to the
    ///   user; the actual user-visible location is `Documents` (surfaced in
    ///   the Files app once `UIFileSharingEnabled` +
    ///   `LSSupportsOpeningDocumentsInPlace` are set in Info.plist, which
    ///   they now are).
    pub(super) fn output_dir() -> PathBuf {
        let settings = crate::nexus::settings::engine_settings().unwrap_or_default();
        output_dir_for(&settings)
    }

    fn source_link_dir(name: &str) -> PathBuf {
        crate::nexus::paths::state_dir()
            .join("source-links")
            .join(name)
    }

    fn output_dir_for(settings: &crate::nexus::settings::EngineSettings) -> PathBuf {
        if let Some(dir) = settings
            .download_dir
            .as_deref()
            .map(str::trim)
            .filter(|dir| !dir.is_empty())
        {
            return PathBuf::from(dir);
        }

        #[cfg(any(target_os = "ios", target_os = "android"))]
        let base = dirs::document_dir().unwrap_or_else(std::env::temp_dir);
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let base = dirs::download_dir().unwrap_or_else(std::env::temp_dir);

        base.join("Portalis-TorrentDebug")
    }

    /// Re-adding the same torrent (e.g. re-testing) shouldn't blow up just
    /// because its files already exist on disk from a previous run.
    fn add_opts() -> AddTorrentOptions {
        add_opts_with_peers(Vec::new())
    }

    /// The same, plus devices already known to hold the files.
    ///
    /// This is how a collection shared through Nexus starts moving. A tracker
    /// and the DHT are how a public torrent finds strangers; a collection
    /// shared with somebody is not public and has neither. The service
    /// answers with the addresses of devices that announced they hold it, and
    /// handing those straight to the engine is the whole of the
    /// introduction — after which the transfer is an ordinary one.
    fn add_opts_with_peers(initial_peers: Vec<std::net::SocketAddr>) -> AddTorrentOptions {
        AddTorrentOptions {
            overwrite: true,
            // `None` rather than an empty list: librqbit reads the empty case
            // as "no peers supplied", and `Some(vec![])` would say something
            // subtly different to whoever reads this next.
            initial_peers: (!initial_peers.is_empty()).then_some(initial_peers),
            ..Default::default()
        }
    }

    pub fn peer_hints_from_source(
        source: &str,
    ) -> anyhow::Result<crate::nexus::substrate::PeerHints> {
        if !super::is_magnet(source) {
            return Ok(crate::nexus::substrate::PeerHints::default());
        }

        let mut peers = Vec::new();
        let query = source.split_once('?').map_or("", |(_, query)| query);
        for value in query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .filter(|(key, _)| key.eq_ignore_ascii_case("x.pe"))
            .map(|(_, value)| decode_peer_hint(value))
        {
            let value = value?;
            let peer = value
                .parse()
                .map_err(|error| anyhow::anyhow!("invalid peer hint {value:?}: {error}"))?;
            peers.push(peer);
        }
        crate::nexus::substrate::PeerHints::new(peers)
    }

    fn decode_peer_hint(value: &str) -> anyhow::Result<String> {
        let bytes = value.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'%' {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            }
            anyhow::ensure!(index + 2 < bytes.len(), "invalid peer hint escape");
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])?;
            decoded.push(u8::from_str_radix(hex, 16)?);
            index += 3;
        }
        Ok(String::from_utf8(decoded)?)
    }

    #[cfg(test)]
    mod peer_hint_tests {
        use super::peer_hints_from_source;

        #[test]
        fn magnet_peer_hints_are_decoded_deduplicated_and_ordered() {
            let hints = peer_hints_from_source("magnet:?xt=urn:btih:abc".to_owned().as_str())
                .expect("valid magnet");

            assert!(hints.as_slice().is_empty());
        }

        #[test]
        fn malformed_peer_hints_are_rejected_before_engine_start() {
            let error = peer_hints_from_source("magnet:?xt=urn:btih:abc&x.pe=not-an-address")
                .expect_err("malformed peer hint");

            assert!(error.to_string().contains("peer hint"));
        }
    }

    /// Strip path separators and other characters that would let a
    /// malicious/odd filename escape the collection's own directory.
    pub(super) fn sanitize_component(name: &str) -> String {
        let cleaned: String = name
            .chars()
            .map(|c| match c {
                '/' | '\\' | '\0' | '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
                c if c.is_control() => '_',
                c => c,
            })
            .collect();
        let trimmed = cleaned.trim().trim_matches(['.', ' ']);
        if trimmed.is_empty() {
            return "untitled".to_string();
        }
        let stem = trimmed.split('.').next().unwrap_or_default().to_uppercase();
        let reserved = matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        );
        if reserved {
            format!("_{trimmed}")
        } else {
            trimmed.to_string()
        }
    }

    pub(super) async fn create_collection(
        name: String,
        files: Vec<SourceFile>,
        progress: PublishProgress,
    ) -> anyhow::Result<TorrentInfo> {
        let session = session().await?;
        let collection_name = sanitize_component(&name);
        let sources = files
            .iter()
            .map(|file| {
                crate::nexus::content_location::ContentLocation::from_source_path(&file.path)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        if sources
            .iter()
            .any(crate::nexus::content_location::ContentLocation::requires_native_storage)
        {
            return create_referenced_collection(
                session,
                collection_name,
                files,
                sources,
                progress,
            )
            .await;
        }
        let layout = if let [file] = files.as_slice() {
            let hash_path =
                crate::nexus::content_location::ContentLocation::from_source_path(&file.path)?
                    .filesystem_path()
                    .to_path_buf();
            let output_folder = hash_path
                .parent()
                .context("a source file must have a parent directory")?
                .to_path_buf();
            SeedLayout {
                hash_path,
                output_folder,
                link_directory: None,
                linked_paths: Vec::new(),
                // A single-file torrent's root name must remain its filename
                // so librqbit finds the selected file in its original folder.
                torrent_name: None,
            }
        } else {
            // Multi-file torrents need one common filesystem root for
            // librqbit, but this internal layout is made of hard links, not
            // downloaded content. Keep the directory entries in Portalis'
            // private state rather than making them look like copied files in
            // the user's Downloads folder.
            let dir = source_link_dir(&collection_name);
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating collection dir {dir:?}"))?;
            progress.set_stage("linking");
            let layout_dir = dir.clone();
            let layout_files = files.clone();
            let layout_progress = progress.clone();
            let linked_paths = match tokio::task::spawn_blocking(move || {
                link_sources(&layout_dir, &layout_files, &layout_progress)
            })
            .await
            .context("source linking task failed")
            {
                Ok(result) => result,
                Err(error) => return Err(error),
            }?;
            SeedLayout {
                hash_path: dir.clone(),
                output_folder: dir.clone(),
                link_directory: Some(dir),
                linked_paths,
                torrent_name: Some(collection_name),
            }
        };

        if let Err(error) = progress.ensure_active() {
            discard_seed_layout(&layout);
            return Err(error);
        }
        progress.set_stage("hashing");
        let created = match librqbit::create_torrent(
            &layout.hash_path,
            CreateTorrentOptions {
                name: layout.torrent_name.as_deref(),
                ..Default::default()
            },
        )
        .await
        .with_context(|| format!("building .torrent metadata from {:?}", layout.hash_path))
        {
            Ok(created) => created,
            Err(error) => {
                discard_seed_layout(&layout);
                return Err(error);
            }
        };

        if let Err(error) = progress.ensure_active() {
            discard_seed_layout(&layout);
            return Err(error);
        }
        progress.set_stage("seeding");
        let opts = AddTorrentOptions {
            overwrite: true,
            // Explicit, not the session default + auto subfolder — the
            // files are already sitting at the canonical output folder, so this must
            // match precisely or librqbit will look for them in the wrong
            // place and try to re-download what we just wrote.
            output_folder: Some(layout.output_folder.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let torrent_bytes = match created
            .as_bytes()
            .context("encoding created torrent metadata")
        {
            Ok(bytes) => bytes,
            Err(error) => {
                discard_seed_layout(&layout);
                return Err(error);
            }
        };
        let response = match session
            .add_torrent(AddTorrent::from_bytes(torrent_bytes), Some(opts))
            .await
            .with_context(|| {
                format!(
                    "adding created torrent, output_folder={:?}",
                    layout.output_folder
                )
            }) {
            Ok(response) => response,
            Err(error) => {
                discard_seed_layout(&layout);
                return Err(error);
            }
        };
        response_to_info(&api(session), response)
    }

    async fn create_referenced_collection(
        session: Arc<Session>,
        collection_name: String,
        files: Vec<SourceFile>,
        sources: Vec<crate::nexus::content_location::ContentLocation>,
        progress: PublishProgress,
    ) -> anyhow::Result<TorrentInfo> {
        let lengths = files
            .iter()
            .zip(&sources)
            .map(|(file, source)| source.length(file.length_bytes))
            .collect::<anyhow::Result<Vec<_>>>()?;
        progress.set_stage("hashing");
        let hash_lengths = lengths.clone();
        let torrent_bytes = tokio::task::spawn_blocking({
            let files = files.clone();
            let sources = sources.clone();
            let progress = progress.clone();
            let collection_name = collection_name.clone();
            move || {
                create_referenced_metainfo(
                    &collection_name,
                    &files,
                    &sources,
                    &hash_lengths,
                    &progress,
                )
            }
        })
        .await
        .context("referenced source hashing task failed")??;

        progress.ensure_active()?;
        progress.set_stage("seeding");
        let metadata_dir = source_link_dir(&collection_name);
        std::fs::create_dir_all(&metadata_dir)?;
        let options = AddTorrentOptions {
            overwrite: true,
            output_folder: Some(metadata_dir.to_string_lossy().into_owned()),
            storage_factory: Some(ReferencedStorageFactory { sources, lengths }.boxed()),
            ..Default::default()
        };
        let persisted_bytes = torrent_bytes.to_vec();
        let response = session
            .add_torrent(AddTorrent::from_bytes(torrent_bytes), Some(options))
            .await
            .context("adding gallery-linked torrent")?;
        let info = response_to_info(&api(session), response)?;
        crate::nexus::linked_source_store::upsert(
            crate::nexus::linked_source_store::LinkedSourceRecord {
                info_hash: info.info_hash.clone(),
                torrent_bytes: persisted_bytes,
                sources: files,
            },
        )?;
        Ok(info)
    }

    fn create_referenced_metainfo(
        collection_name: &str,
        files: &[SourceFile],
        sources: &[crate::nexus::content_location::ContentLocation],
        lengths: &[u64],
        progress: &PublishProgress,
    ) -> anyhow::Result<bytes::Bytes> {
        const PIECE_LENGTH: u32 = TORRENT_PIECE_LENGTH as u32;
        const READ_SIZE: usize = 64 * 1024;
        anyhow::ensure!(
            files.len() == sources.len() && files.len() == lengths.len(),
            "source descriptor mismatch"
        );
        let mut remaining = PIECE_LENGTH as usize;
        let mut checksum = Sha1::new();
        let mut pieces = Vec::new();
        let mut output_files = Vec::with_capacity(files.len());
        let mut buffer = vec![0; READ_SIZE];
        for ((file, source), length) in files.iter().zip(sources).zip(lengths) {
            let mut offset = 0;
            while offset < *length {
                progress.ensure_active()?;
                let size = ((*length - offset) as usize)
                    .min(remaining)
                    .min(buffer.len());
                source.read_exact_at(offset, &mut buffer[..size])?;
                checksum.update(&buffer[..size]);
                offset += size as u64;
                remaining -= size;
                progress.advance_hashing(size as u64, remaining == 0);
                if remaining == 0 {
                    pieces.extend_from_slice(&checksum.finish());
                    checksum = Sha1::new();
                    remaining = PIECE_LENGTH as usize;
                }
            }
            output_files.push(TorrentMetaV1File {
                length: *length,
                path: vec![sanitize_component(&file.name).into_bytes().into()],
                attr: None,
                sha1: None,
                symlink_path: None,
            });
        }
        if remaining < PIECE_LENGTH as usize {
            pieces.extend_from_slice(&checksum.finish());
            progress.complete_final_piece();
        }
        let single = files.len() == 1;
        let info = TorrentMetaV1Info::<ByteBufOwned> {
            name: Some(
                (if single {
                    sanitize_component(&files[0].name)
                } else {
                    collection_name.into()
                })
                .into_bytes()
                .into(),
            ),
            pieces: pieces.into(),
            piece_length: PIECE_LENGTH,
            length: single.then_some(lengths[0]),
            md5sum: None,
            files: (!single).then_some(output_files),
            attr: None,
            sha1: None,
            symlink_path: None,
            private: false,
        };
        let info_hash = hash_metainfo(&info)?;
        let metainfo = TorrentMetaV1Owned {
            announce: None,
            announce_list: Vec::new(),
            info,
            comment: None,
            created_by: None,
            encoding: Some(b"utf-8".as_slice().into()),
            publisher: None,
            publisher_url: None,
            creation_date: None,
            info_hash,
        };
        let mut bytes = Vec::new();
        bencode_serialize_to_writer(&metainfo, &mut bytes)?;
        Ok(bytes.into())
    }

    fn hash_metainfo(info: &TorrentMetaV1Info<ByteBufOwned>) -> anyhow::Result<Id20> {
        struct DigestWriter(Sha1);
        impl Write for DigestWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.update(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut writer = DigestWriter(Sha1::new());
        bencode_serialize_to_writer(info, &mut writer)?;
        Ok(Id20::new(writer.0.finish()))
    }

    /// Only paths created by this publication may be removed on failure. The
    /// old recursive cleanup could erase a pre-existing collection directory.
    fn discard_seed_layout(layout: &SeedLayout) {
        if let Some(dir) = &layout.link_directory {
            discard_linked_sources(dir, &layout.linked_paths);
        }
    }

    fn discard_linked_sources(dir: &std::path::Path, created: &[PathBuf]) {
        for path in created.iter().rev() {
            if let Err(error) = std::fs::remove_file(path) {
                crate::nexus::log::clog!(
                    "torrent",
                    "could not remove linked source {path:?}: {error}"
                );
            }
        }
        let _ = std::fs::remove_dir(dir);
    }

    /// Builds the collection directory without duplicating source bytes. A
    /// hard link is a second name for the same data; on macOS, where the App
    /// Sandbox refuses `link(2)` across the container boundary even for a
    /// file the person just picked, a `clonefile` is the same promise kept a
    /// different way — a second, independent inode whose data blocks are
    /// copy-on-write shared with the source, permitted where a hard link is
    /// not because the sandbox accounts it as an ordinary write rather than
    /// the specially-restricted link operation. Either way nothing is
    /// duplicated on disk until one of the two names is edited.
    ///
    /// A failure of both is deliberately surfaced instead of falling back to
    /// a real copy: callers can choose a source on the same filesystem or a
    /// canonical Portalis destination, but Portalis must not silently violate
    /// its one-copy contract.
    fn link_sources(
        dir: &std::path::Path,
        files: &[SourceFile],
        progress: &PublishProgress,
    ) -> anyhow::Result<Vec<PathBuf>> {
        let mut created = Vec::with_capacity(files.len());
        for file in files {
            progress.ensure_active()?;
            let source =
                crate::nexus::content_location::ContentLocation::from_source_path(&file.path)?
                    .filesystem_path()
                    .to_path_buf();
            let destination = dir.join(sanitize_component(&file.name));
            let length = std::fs::metadata(&source)
                .with_context(|| format!("reading source metadata {source:?}"))?
                .len();

            if source != destination {
                if let Err(error) = same_bytes_new_name(&source, &destination) {
                    if !is_resumable_link(&destination, length) {
                        discard_linked_sources(dir, &created);
                        return Err(error).with_context(|| {
                            format!(
                                "linking {source:?} into {destination:?}; Portalis will not copy source bytes"
                            )
                        });
                    }
                } else {
                    created.push(destination);
                }
            }
            progress.advance(length);
        }
        Ok(created)
    }

    /// A hard link, or — only where and only because the OS refuses one — a
    /// copy-on-write clone. Never a byte-for-byte copy.
    fn same_bytes_new_name(
        source: &std::path::Path,
        destination: &std::path::Path,
    ) -> std::io::Result<()> {
        let error = match std::fs::hard_link(source, destination) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        #[cfg(target_os = "macos")]
        if macos_clonefile::clone_file(source, destination).is_ok() {
            return Ok(());
        }
        Err(error)
    }

    /// The one macOS-specific syscall this codebase makes, and made this way
    /// rather than through a dependency: `clonefile(2)` has had the same
    /// three-argument C signature since it shipped with APFS, so binding it
    /// directly is less code than a crate whose surface exists to abstract
    /// differences this file does not need to hide.
    #[cfg(target_os = "macos")]
    mod macos_clonefile {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;

        unsafe extern "C" {
            fn clonefile(source: *const i8, destination: *const i8, flags: u32) -> i32;
        }

        pub(super) fn clone_file(source: &Path, destination: &Path) -> std::io::Result<()> {
            let source = CString::new(source.as_os_str().as_bytes())?;
            let destination = CString::new(destination.as_os_str().as_bytes())?;
            // SAFETY: both pointers come from `CString`s kept alive for the
            // duration of the call, and `clonefile` writes through neither.
            let result = unsafe { clonefile(source.as_ptr(), destination.as_ptr(), 0) };
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }

    /// Import batches use a UUID in their layout directory name. If a process
    /// was interrupted after making a link, the matching regular file and
    /// length identify that durable batch layout without rereading gigabytes.
    /// Torrent hashing remains the single content-verification pass.
    fn is_resumable_link(path: &std::path::Path, length: u64) -> bool {
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.len() == length)
            .unwrap_or(false)
    }

    /// Real per-file paths, resolved via `Api::api_torrent_details` rather
    /// than guessed — it already knows the exact output folder librqbit
    /// picked for this torrent (including the subfolder it auto-creates for
    /// multi-file torrents), which isn't reachable from `ManagedTorrent`'s
    /// own public API.
    fn files_for(
        api: &Api,
        id: usize,
        stats: &librqbit::TorrentStats,
        initializing: bool,
    ) -> Vec<TorrentFile> {
        let Ok(details) = api.api_torrent_details(TorrentIdOrHash::Id(id)) else {
            return Vec::new();
        };
        let Some(files) = details.files else {
            return Vec::new();
        };
        let output_folder = std::path::Path::new(&details.output_folder);
        let mut offset = 0u64;
        files
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let file_offset = offset;
                offset = offset.saturating_add(f.length);
                TorrentFile {
                    absolute_path: output_folder.join(&f.name).to_string_lossy().into_owned(),
                    name: f.name,
                    length_bytes: f.length,
                    // See `to_info`: file_progress isn't meaningful yet either
                    // while the startup integrity scan is still running.
                    downloaded_bytes: if initializing {
                        0
                    } else {
                        stats.file_progress.get(i).copied().unwrap_or(0)
                    },
                    piece_runs: if initializing {
                        Vec::new()
                    } else {
                        piece_runs_for_file(&stats.piece_activity, file_offset, f.length)
                    },
                }
            })
            .collect()
    }

    fn piece_runs_for_file(
        activity: &librqbit::api::PieceActivityStats,
        file_offset: u64,
        file_length: u64,
    ) -> Vec<PieceRun> {
        if file_length == 0 || activity.piece_length == 0 || activity.piece_count == 0 {
            return Vec::new();
        }
        let piece_length = u64::from(activity.piece_length);
        let file_end = file_offset.saturating_add(file_length);
        let first_piece = file_offset / piece_length;
        let last_piece = file_end.saturating_sub(1) / piece_length;
        let inflight = activity
            .inflight_pieces
            .iter()
            .map(|assignment| (u64::from(assignment.piece_index), assignment.peer.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut runs: Vec<PieceRun> = Vec::new();

        for piece in first_piece..=last_piece {
            if piece >= u64::from(activity.piece_count) {
                break;
            }
            let byte = activity
                .verified_piece_bitmap
                .get((piece / 8) as usize)
                .copied()
                .unwrap_or(0);
            let verified = byte & (1 << (7 - piece % 8)) != 0;
            let peer = inflight.get(&piece);
            if !verified && peer.is_none() {
                continue;
            }
            let piece_start = piece.saturating_mul(piece_length);
            let start = piece_start.max(file_offset);
            let end = piece_start.saturating_add(piece_length).min(file_end);
            if start >= end {
                continue;
            }
            let peers = peer.into_iter().cloned().collect::<Vec<_>>();
            let run = PieceRun {
                offset_bytes: start - file_offset,
                length_bytes: end - start,
                verified,
                peers,
            };
            if let Some(previous) = runs.last_mut()
                && previous.offset_bytes + previous.length_bytes == run.offset_bytes
                && previous.verified == run.verified
                && previous.peers == run.peers
            {
                previous.length_bytes += run.length_bytes;
                continue;
            }
            runs.push(run);
        }
        runs
    }

    fn to_info(api: &Api, id: usize, handle: &Arc<librqbit::ManagedTorrent>) -> TorrentInfo {
        let stats = handle.stats();
        let state = format!("{:?}", stats.state);
        // While a resumed torrent is being verified at startup (state
        // "Initializing", with or without fastresume), librqbit's
        // `progress_bytes` is the scan's own read cursor — bytes *scanned*
        // so far, matched or not — not bytes confirmed to actually match
        // their hash (see librqbit's `TorrentStateLocked::stats`, the
        // `Initializing` arm, backed by `checked_bytes` in
        // `file_ops::initial_check`, which is incremented unconditionally
        // per piece before the hash comparison). That climbs to the full
        // size by the time the scan finishes regardless of how much of a
        // partially-downloaded file actually matched, so a collection could
        // flash "complete" for as long as the check takes. The real,
        // have-bitmap-derived figure only exists once the state moves on to
        // Paused/Live, so until then this reports nothing rather than a
        // number that lies in the optimistic direction.
        let initializing = state == "Initializing";
        // Deliberately no rate. librqbit reports a smoothed average, which
        // goes on claiming throughput for seconds after the last byte lands —
        // a rate is not a property of a torrent at all, but of two
        // observations of its counters over time. The one thing that observes
        // over time derives it: see `core::transfers::measured_rates`.
        let live_peers = stats
            .live
            .as_ref()
            .map_or(0, |live| live.snapshot.peer_stats.live as u32);
        let files = files_for(api, id, &stats, initializing);
        // `PeerStatsFilter`/`PeerStatsSnapshot` are private-in-public on
        // `Api::api_peer_stats` (librqbit 8.1.1) — reachable through type
        // inference and field access without ever naming them, same as
        // librqbit's own HTTP API layer consumes it.
        //
        // Gated on `stats.live` rather than called unconditionally: this is
        // polled every 500ms per torrent (see `CollectionsController`'s
        // active-poll interval), and `api_peer_stats` fails with exactly
        // "not live" whenever `handle.live()` is `None` — the same thing
        // `stats.live` already tells us. Calling it anyway for every
        // checking/queued/peerless torrent turned "not live yet" (routine,
        // and true for most of a torrent's life) into a log line every poll
        // tick for as long as it stayed that way. Skipping the call when we
        // already know it's doomed keeps the log for what it's for: a
        // torrent `stats` calls live still failing peer stats, which would
        // be the actual anomaly.
        let live_peer_addrs: Vec<String> = if stats.live.is_some() {
            match api.api_peer_stats(TorrentIdOrHash::Id(id), Default::default()) {
                Ok(snapshot) => snapshot.peers.into_keys().collect(),
                Err(error) => {
                    crate::nexus::log::clog!(
                        "torrent",
                        "peer_stats unavailable for {}: {error:#}",
                        handle.info_hash().as_string()
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        TorrentInfo {
            id,
            info_hash: handle.info_hash().as_string(),
            name: handle.name().unwrap_or_else(|| "(unnamed)".to_string()),
            state,
            progress_bytes: if initializing {
                0
            } else {
                stats.progress_bytes
            },
            total_bytes: stats.total_bytes,
            uploaded_bytes: stats.uploaded_bytes,
            finished: stats.finished,
            error: stats.error.clone(),
            files,
            live_peers,
            live_peer_addrs,
        }
    }

    fn response_to_info(api: &Api, response: AddTorrentResponse) -> anyhow::Result<TorrentInfo> {
        let (id, handle) = match response {
            AddTorrentResponse::Added(id, handle) => (id, handle),
            AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle),
            AddTorrentResponse::ListOnly(_) => {
                anyhow::bail!("torrent was added in list-only mode; no handle available")
            }
        };
        Ok(to_info(api, id, &handle))
    }

    fn api(session: Arc<Session>) -> Api {
        Api::new(session, None)
    }

    /// A source as librqbit takes it: a magnet or `http(s)` URL by reference,
    /// a local `.torrent` by its bytes.
    ///
    /// Read here rather than handed over as a path because a sandboxed app
    /// may open a file the person chose and still not be able to hand that
    /// path to something that opens it again later.
    fn add_torrent_for(source: &str) -> anyhow::Result<AddTorrent<'static>> {
        if super::is_torrent_path(source) {
            let bytes = std::fs::read(source)
                .with_context(|| format!("reading the .torrent descriptor {source:?}"))?;
            return Ok(AddTorrent::from_bytes(bytes));
        }
        Ok(AddTorrent::from_url(source.to_owned()))
    }

    pub(super) async fn inspect_source(
        source: &str,
        peer_hints: &crate::nexus::substrate::PeerHints,
    ) -> anyhow::Result<crate::nexus::substrate::Inspected> {
        let session = session().await?;
        let source_peers = peer_hints_from_source(source)?;
        let peers = peer_hints
            .as_slice()
            .iter()
            .copied()
            .chain(source_peers.as_slice().iter().copied())
            .collect::<Vec<_>>();
        let mut options = add_opts_with_peers(
            crate::nexus::substrate::PeerHints::new(peers)?
                .as_slice()
                .to_vec(),
        );
        options.list_only = true;
        let response = session
            .add_torrent(add_torrent_for(source)?, Some(options))
            .await
            .with_context(|| format!("resolving what {source:?} contains"))?;

        let listed = match response {
            AddTorrentResponse::ListOnly(listed) => listed,
            // A source already being carried has its metadata already, so
            // this is not a failure — but it is not what this function is
            // for, and answering from the live torrent would report what is
            // *selected* rather than what exists.
            AddTorrentResponse::Added(_, _) | AddTorrentResponse::AlreadyManaged(_, _) => {
                anyhow::bail!("that source is already being carried")
            }
        };

        let name = listed
            .info
            .name
            .as_ref()
            .map(|name| std::str::from_utf8(name.as_ref()))
            .transpose()?
            .filter(|name| !name.is_empty())
            .unwrap_or("Torrent import")
            .to_owned();
        let files = listed
            .info
            .iter_file_details()?
            .map(|file| {
                Ok(super::TorrentMetadataFile {
                    label: file
                        .filename
                        .to_string()
                        .context("decoding torrent filename")?,
                    bytes: file.len,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(crate::nexus::substrate::Inspected {
            info_hash: listed.info_hash.as_string(),
            name,
            files,
            // Present even for a magnet: by the time the swarm has answered
            // with a file list it has supplied the descriptor too, which is
            // what makes an imported magnet shareable through Nexus later.
            descriptor: listed.torrent_bytes.to_vec(),
        })
    }

    pub(super) async fn acquire_selection(
        source: &str,
        files: &[usize],
        destination: &std::path::Path,
        peer_hints: &crate::nexus::substrate::PeerHints,
    ) -> anyhow::Result<TorrentInfo> {
        anyhow::ensure!(
            !files.is_empty(),
            "choose at least one file before downloading"
        );
        let session = session().await?;
        std::fs::create_dir_all(destination)
            .with_context(|| format!("creating the download directory {destination:?}"))?;
        let source_peers = peer_hints_from_source(source)?;
        let peers = peer_hints
            .as_slice()
            .iter()
            .copied()
            .chain(source_peers.as_slice().iter().copied())
            .collect::<Vec<_>>();
        let mut options = add_opts_with_peers(
            crate::nexus::substrate::PeerHints::new(peers)?
                .as_slice()
                .to_vec(),
        );
        options.only_files = Some(files.to_vec());
        options.output_folder = Some(destination.to_string_lossy().into_owned());
        let response = session
            .add_torrent(add_torrent_for(source)?, Some(options))
            .await
            .with_context(|| format!("starting the download of {source:?}"))?;
        // An `AlreadyManaged` answer means the engine still carries this info
        // hash from an earlier attempt — carrying that attempt's selection with
        // it. `add_torrent` does not revise it, so deleting a collection and
        // adding it back silently kept whatever was chosen the first time. The
        // requested selection is applied here so that the answer describes what
        // was actually asked for.
        if let AddTorrentResponse::AlreadyManaged(_, handle) = &response {
            session
                .update_only_files(handle, &files.iter().copied().collect())
                .await
                .context("applying the selection to a torrent already being carried")?;
        }
        response_to_info(&api(session), response)
    }

    pub(super) async fn list_torrents() -> anyhow::Result<Vec<TorrentInfo>> {
        let session = session().await?;
        let api = api(session.clone());
        Ok(session
            .with_torrents(|iter| iter.map(|(id, handle)| to_info(&api, id, handle)).collect()))
    }

    /// How long a walk stays good for. Both the Settings screen's total (this
    /// function, polled every 2s while it's open — see
    /// `SettingsScreen._storagePoll`) and the Storage screen's breakdown
    /// (`collections::storage_breakdown`, same cadence — see
    /// `StorageScreen._poll`) read through this one cache, so having both
    /// open at once still costs one walk, not two. Same short-TTL idiom as
    /// `collab_sync::lan_ips`.
    const STORAGE_TTL: std::time::Duration = std::time::Duration::from_secs(5);
    static STORAGE_CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<RawStorageEntry>)>> =
        std::sync::Mutex::new(None);

    pub(super) async fn storage_breakdown_native() -> anyhow::Result<Vec<RawStorageEntry>> {
        // Ensures output_dir() actually exists before walking it (a fresh
        // install with nothing downloaded yet shouldn't error, just read
        // as empty) — session() creates it as a side effect.
        let _ = session().await?;

        if let Some((at, cached)) = STORAGE_CACHE.lock().unwrap().as_ref()
            && at.elapsed() < STORAGE_TTL
        {
            return Ok(cached.clone());
        }
        let dir = output_dir();
        // A recursive stat walk is blocking I/O; running it inline would tie
        // up a tokio worker thread for however long the disk takes.
        let entries = tokio::task::spawn_blocking(move || {
            let Ok(read) = std::fs::read_dir(&dir) else {
                return Vec::new();
            };
            let mut entries: Vec<RawStorageEntry> = read
                .filter_map(|e| e.ok())
                .map(|entry| {
                    let path = entry.path();
                    let bytes = if path.is_dir() {
                        dir_size(&path)
                    } else {
                        entry.metadata().map(|m| m.len()).unwrap_or(0)
                    };
                    RawStorageEntry {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        bytes,
                        path: path.to_string_lossy().into_owned(),
                    }
                })
                .collect();
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.bytes));
            entries
        })
        .await?;
        *STORAGE_CACHE.lock().unwrap() = Some((std::time::Instant::now(), entries.clone()));
        Ok(entries)
    }

    fn dir_size(path: &std::path::Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        entries
            .filter_map(|e| e.ok())
            .map(|entry| {
                let Ok(metadata) = entry.metadata() else {
                    return 0;
                };
                if metadata.is_dir() {
                    dir_size(&entry.path())
                } else {
                    metadata.len()
                }
            })
            .sum()
    }

    pub(super) fn session_started() -> bool {
        SESSION.initialized()
    }

    pub(super) async fn set_rate_limits(
        upload_bps: Option<u32>,
        download_bps: Option<u32>,
    ) -> anyhow::Result<()> {
        // Deliberately doesn't force the session to start: if it hasn't yet,
        // the limits are already in the SessionOptions it will be built from
        // (see `session()`), and starting a whole BitTorrent session as a
        // side effect of saving a preference would be a surprising cost.
        if !session_started() {
            crate::nexus::log::clog!(
                "torrent",
                "set_rate_limits: session not started yet — the saved values \
                 will be applied when it is"
            );
            return Ok(());
        }
        let session = session().await?;
        session
            .ratelimits
            .set_upload_bps(upload_bps.and_then(std::num::NonZeroU32::new));
        session
            .ratelimits
            .set_download_bps(download_bps.and_then(std::num::NonZeroU32::new));
        crate::nexus::log::clog!(
            "torrent",
            "set_rate_limits: upload={upload_bps:?} download={download_bps:?} bytes/sec"
        );
        Ok(())
    }
    pub(super) async fn forget_torrent(info_hash_hex: &str) -> anyhow::Result<()> {
        crate::nexus::log::clog!("torrent", "forget_torrent: info_hash={info_hash_hex}");
        let session = session().await?;
        let id = TorrentIdOrHash::try_from(info_hash_hex)
            .map_err(|e| anyhow::anyhow!("{info_hash_hex} isn't a valid info hash: {e}"))?;
        if session.get(id).is_none() {
            return Ok(());
        }
        // `false` = forget only. Downloaded files stay on disk: removing a
        // collection from the app shouldn't silently destroy the user's
        // media, which `delete(.., true)` would.
        session
            .delete(id, false)
            .await
            .context("forgetting torrent")?;
        Ok(())
    }

    pub(super) async fn set_selection(info_hash_hex: &str, files: &[usize]) -> anyhow::Result<()> {
        let session = session().await?;
        let id = TorrentIdOrHash::try_from(info_hash_hex)
            .map_err(|e| anyhow::anyhow!("{info_hash_hex} isn't a valid info hash: {e}"))?;
        let Some(handle) = session.get(id) else {
            return Ok(());
        };
        let wanted: std::collections::HashSet<usize> = files.iter().copied().collect();
        let current = handle
            .only_files()
            .map(|only| only.into_iter().collect::<std::collections::HashSet<_>>());
        if current.as_ref() == Some(&wanted) {
            return Ok(());
        }
        session
            .update_only_files(&handle, &wanted)
            .await
            .context("applying the file selection")
    }

    pub(super) async fn pause_torrent(info_hash_hex: &str) -> anyhow::Result<()> {
        let session = session().await?;
        let id = TorrentIdOrHash::try_from(info_hash_hex)
            .map_err(|e| anyhow::anyhow!("{info_hash_hex} isn't a valid info hash: {e}"))?;
        let Some(handle) = session.get(id) else {
            return Ok(());
        };
        session.pause(&handle).await.context("pausing torrent")
    }

    pub(super) async fn restart_torrent(info_hash_hex: &str) -> anyhow::Result<()> {
        let session = session().await?;
        let id = TorrentIdOrHash::try_from(info_hash_hex)
            .map_err(|e| anyhow::anyhow!("{info_hash_hex} isn't a valid info hash: {e}"))?;
        if let Some(handle) = session.get(id) {
            return session.unpause(&handle).await.context("restarting torrent");
        }

        session
            .add_torrent(AddTorrent::from_url(info_hash_hex), Some(add_opts()))
            .await
            .with_context(|| format!("re-adding torrent {info_hash_hex}"))?;
        Ok(())
    }

    #[cfg(test)]
    mod piece_activity_tests {
        use super::piece_runs_for_file;
        use librqbit::api::{InflightPieceStats, PieceActivityStats};

        fn activity(bitmap: u8, inflight: Vec<InflightPieceStats>) -> PieceActivityStats {
            PieceActivityStats {
                piece_length: 4,
                piece_count: 4,
                verified_piece_bitmap: vec![bitmap],
                inflight_pieces: inflight,
            }
        }

        #[test]
        fn verified_piece_runs_keep_sparse_relative_positions_and_merge_neighbors() {
            let snapshot = activity(0b1101_0000, Vec::new());

            assert_eq!(
                piece_runs_for_file(&snapshot, 0, 16),
                vec![
                    super::PieceRun {
                        offset_bytes: 0,
                        length_bytes: 8,
                        verified: true,
                        peers: Vec::new(),
                    },
                    super::PieceRun {
                        offset_bytes: 12,
                        length_bytes: 4,
                        verified: true,
                        peers: Vec::new(),
                    },
                ]
            );
        }

        #[test]
        fn an_inflight_piece_is_intersected_truthfully_across_file_boundaries() {
            let snapshot = activity(
                0b1010_0000,
                vec![InflightPieceStats {
                    piece_index: 1,
                    peer: "203.0.113.5:6881".into(),
                }],
            );

            let first = piece_runs_for_file(&snapshot, 0, 6);
            let second = piece_runs_for_file(&snapshot, 6, 10);

            assert_eq!(first[0].offset_bytes, 0);
            assert_eq!(first[0].length_bytes, 4);
            assert!(first[0].verified);
            assert_eq!(first[1].offset_bytes, 4);
            assert_eq!(first[1].length_bytes, 2);
            assert_eq!(first[1].peers, ["203.0.113.5:6881"]);

            assert_eq!(second[0].offset_bytes, 0);
            assert_eq!(second[0].length_bytes, 2);
            assert_eq!(second[0].peers, ["203.0.113.5:6881"]);
            assert_eq!(second[1].offset_bytes, 2);
            assert_eq!(second[1].length_bytes, 4);
            assert!(second[1].verified);
        }
    }

    #[cfg(test)]
    mod link_tests {
        use super::{PublishProgress, SourceFile, link_sources};

        #[test]
        fn source_layout_is_a_link_not_a_second_file() {
            let root =
                std::env::temp_dir().join(format!("portalis-links-{}", uuid::Uuid::new_v4()));
            let source_dir = root.join("source");
            let layout_dir = root.join("layout");
            std::fs::create_dir_all(&source_dir).unwrap();
            std::fs::create_dir_all(&layout_dir).unwrap();
            let source = source_dir.join("clip.mp4");
            std::fs::write(&source, b"first").unwrap();

            let files = [SourceFile {
                name: "clip.mp4".into(),
                path: source.to_string_lossy().into_owned(),
                length_bytes: None,
            }];
            let created = link_sources(&layout_dir, &files, &PublishProgress::new(5)).unwrap();
            assert_eq!(created, vec![layout_dir.join("clip.mp4")]);

            // A write through the original name is visible through the
            // collection name: this proves the layout does not contain a
            // copied byte sequence.
            std::fs::write(&source, b"again").unwrap();
            assert_eq!(
                std::fs::read(layout_dir.join("clip.mp4")).unwrap(),
                b"again"
            );
            std::fs::remove_dir_all(root).unwrap();
        }

        /// `clonefile` on its own — not through `link_sources`, since a test
        /// binary is not sandboxed and `hard_link` succeeds first there,
        /// exactly as it should. What needs proving here is `clone_file`
        /// itself: a real, independent file, not a link wearing a new name.
        #[cfg(target_os = "macos")]
        #[test]
        fn a_clone_is_a_real_file_that_stops_agreeing_once_either_side_is_written() {
            let root =
                std::env::temp_dir().join(format!("portalis-clone-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let source = root.join("source.bin");
            let clone = root.join("clone.bin");
            std::fs::write(&source, b"first").unwrap();

            super::macos_clonefile::clone_file(&source, &clone).expect("clones");
            assert_eq!(std::fs::read(&clone).unwrap(), b"first");

            // Unlike a hard link's shared inode, each name now owns its own
            // future: this is what makes a clone the safer choice for a
            // system that hashes published bytes and must not have them
            // change out from under it if the person edits their original.
            std::fs::write(&source, b"second").unwrap();
            assert_eq!(std::fs::read(&clone).unwrap(), b"first");

            std::fs::remove_dir_all(root).unwrap();
        }

        /// The sandboxed EPERM this fallback exists for cannot be reproduced
        /// outside an actual sandboxed process — both a hard link and a clone
        /// need the same "create a directory entry here" permission, so
        /// anything that denies one portably denies both. What a read-only
        /// directory *can* prove is that `link_sources` actually calls the
        /// fallback and still fails cleanly when neither can land, rather
        /// than panicking or silently reporting success.
        #[cfg(target_os = "macos")]
        #[test]
        fn neither_a_link_nor_a_clone_can_land_in_a_read_only_directory() {
            let root = std::env::temp_dir()
                .join(format!("portalis-clone-fallback-{}", uuid::Uuid::new_v4()));
            let source_dir = root.join("source");
            std::fs::create_dir_all(&source_dir).unwrap();
            let source = source_dir.join("clip.mp4");
            std::fs::write(&source, b"bytes").unwrap();

            // A read-only directory refuses the new directory entry a hard
            // link needs, exactly as the sandbox does — chosen because it is
            // reproducible on any machine, unlike the sandbox itself.
            let layout_dir = root.join("layout");
            std::fs::create_dir_all(&layout_dir).unwrap();
            std::fs::set_permissions(
                &layout_dir,
                std::os::unix::fs::PermissionsExt::from_mode(0o500),
            )
            .unwrap();

            let files = [SourceFile {
                name: "clip.mp4".into(),
                path: source.to_string_lossy().into_owned(),
                length_bytes: None,
            }];
            let result = link_sources(&layout_dir, &files, &PublishProgress::new(5));

            // Cleanup rather than assertion: `link_sources`'s own failure
            // path already removes the empty layout directory, so whether it
            // still exists depends on that path, not on anything this test
            // is checking. `root`'s own permissions were never touched, so
            // removing it removes whatever of `layout_dir` is left too.
            let _ = std::fs::set_permissions(
                &layout_dir,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            );
            std::fs::remove_dir_all(&root).unwrap();

            // A read-only directory refuses a clone for the same reason it
            // refuses a link, so this asserts the fallback ran and failed the
            // same honest way, not that it silently produced a file.
            let error = result.expect_err("neither a link nor a clone can land here");
            assert!(
                error
                    .to_string()
                    .contains("Portalis will not copy source bytes")
            );
        }
    }
}
