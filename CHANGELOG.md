# Changelog

## 1.0.42+43

- Bumped the coordinated Flutter release to `1.0.42+43` and the Rust backend
  to `0.1.40`.
- Swarm peers now carry what they have actually exchanged. librqbit already
  counted bytes received and sent per peer and announced each peer's client
  name; the engine was discarding all of it and keeping only the address.
  Each connection now reports its own totals, its live up/down rates measured
  between polls, and the name the peer reported for itself.
- People shows those connections. It previously rendered contacts only, and
  the contact list is always empty because nothing writes one yet, so the
  screen could never show anything. Contacts and connections are separate
  sections with different shapes: a contact is somebody whose fingerprint can
  be compared, a connection is an address, and a self-reported client name is
  labelled as reported rather than shown as an identity.
- A peer's rate is the difference between two polls rather than its share of
  the connection's lifetime average, so a peer that delivered a burst and went
  quiet reads as idle instead of still working.
- Removed the drag-to-resize handle under a collection's file grid. The grid
  sizes to its content and the page scrolls; the handle existed only to manage
  a scroll region nested inside another scroll region. The file list now has a
  count header like every other section on the page.

## 1.0.41+42

- Bumped the coordinated Flutter release to `1.0.41+42` and the Rust backend
  to `0.1.39`.
- Store schema 10 replaces the independently persisted `draft` and `paused`
  booleans with one exhaustive durable lifecycle: native draft, native
  published, torrent resolving, torrent awaiting selection, or torrent
  requested. Running or paused activity exists only inside executable states,
  so combinations such as a paused draft can no longer be encoded.
- Schema 9 collections migrate atomically with the schema-version write. The
  migration reads torrent metadata and selected entries alongside each legacy
  row; resolved drafts are deliberately migrated to awaiting selection rather
  than treated as download consent. A falsification test confirms that mapping
  fails if changed to an executable state.
- Metadata resolution now persists and displays as active preparation. Once
  the file list resolves, the collection becomes a draft waiting for the
  explicit Download action. Restarting in either state preserves that exact
  intent and cannot start content acquisition.
- The generic native `PublishDraft` command is rejected for torrent imports.
  Only `DownloadSelection` can move an awaiting torrent to requested, after it
  validates and durably records a non-empty explicit selection; default checked
  entries are never treated as authorization.
- Status projection now accepts the typed durable lifecycle plus live engine
  observations, not a second pair of `draft` and `paused` booleans. Worker,
  publisher, restart hydration, transfer reconciliation, and UI status all
  therefore consume the same stored intent.
- Confirming a draft is now refused for torrent imports, so the generic publish
  action cannot promote an inspected-but-unconfirmed import to executable and
  bypass the explicit Download decision.

## 1.0.40+41

- Bumped the coordinated Flutter release to `1.0.40+41` and the Rust backend
  to `0.1.38`.
- What a collection *is* is now one typed answer rather than a set of loose
  flags each worker reassembled for itself. `core/lifecycle.rs` reads the
  stored row once and reports whether a collection is a draft, resolving,
  waiting for the person to press Download, or genuinely requested — and
  whether the person's pause means anything for it yet. The torrent worker,
  the publisher, the projection rebuild, and every status refresh now ask that
  one question instead of interpreting `draft` and `paused` independently,
  which is what let them disagree.
- A pause recorded against something that cannot transfer no longer names a
  decision. It was stored beside `draft`, ignored while the collection was a
  draft, and could resurface once the collection was confirmed.

## 1.0.39+40

- Bumped the coordinated Flutter release to `1.0.39+40` and the Rust backend
  to `0.1.37`.
- The words the backend sends the interface are now an explicit contract
  instead of whatever `#[derive(Debug)]` happened to print. Every status,
  nature, role, friendship, and connectivity value is spelled out by hand in
  `projection/wire.rs`, so adding or renaming one fails to compile until its
  word is chosen deliberately. Flutter parses those words once, in
  `CollectionState`, rather than comparing string literals in a dozen widgets.
- Fixed three comparisons that had silently rotted and could never match:
  two tested for `'downloading'` and one for `'importing'`, neither of which
  the backend has ever emitted. A downloading collection now shows its
  progress percentage in the list, and a torrent still resolving its file list
  says so instead of claiming to be empty.
- A status word the running build does not recognise is now reported as
  unknown and shown verbatim, rather than silently answering "no" to every
  question the interface asks about that collection.
- Connectivity and friendship no longer cross the bridge as debug-formatted
  Rust values: `Online(Security { .. })` is now simply `Online`.

## 1.0.38+39

- Bumped the coordinated Flutter release to `1.0.38+39` and the Rust backend
  to `0.1.36`.
- Releasing a torrent from the session no longer forgets where the person's
  original media lives. The transfer poller releases any torrent no collection
  currently claims, and that purged the source references — so a collection
  briefly unclaimed during startup lost the only record of its originals, and
  the next launch reported its files as living under the download folder. Only
  deleting the collection forgets them now, which is the one moment they stop
  being wanted.

## 1.0.37+38

- Bumped the coordinated Flutter release to `1.0.37+38` and the Rust backend
  to `0.1.35`.
- A resolved torrent import is no longer acquired until the person presses
  Download. Resolving selects every file so the selection screen opens with
  something in it, and that default was being read as a request: because
  opening the app wakes the import worker, reopening Portalis started
  downloading a torrent that had only ever been inspected.
- Restored shares now read the person's own files again. Rehydration was
  leaving librqbit's output folder unset, so a reopened collection fell back to
  the download directory instead of the info-hash-keyed metadata directory
  publication had chosen, and two collections sharing a torrent name could
  overwrite one another's state there. Resuming a share the session had
  dropped rebuilds it from the retained source references for the same reason,
  rather than re-adding it by info hash and fetching content this device
  already holds.
- One unreadable source no longer stops every later collection from being
  restored: rehydration failures are now contained per collection instead of
  abandoning the whole startup pass.
- Restored torrents are added paused and started by the reconciler, so a
  collection the person stopped never moves bytes during the window between
  session startup and the first reconcile pass.
- macOS now keeps security-scoped bookmarks for selected sources, matching what
  iOS already does. A sandboxed app loses access to a chosen file when the
  process that chose it exits, and because Portalis seeds the original file
  rather than a copy, reopening the app failed with `Operation not permitted`
  and stopped seeding everything it had shared.
- Per-file source paths are matched by name rather than by list position when
  a restored torrent reports its files, so a multi-file collection cannot show
  one file's original location beside another file's name.

## 1.0.36+37

- Bumped the coordinated Flutter release to `1.0.36+37` and the Rust backend
  to `0.1.34`.
- Live collection speed now uses librqbit's native, per-torrent
  `fetched_bytes` counter over the actual polling interval. This shows peer
  receive activity before a whole piece has completed hash verification, while
  collection progress, completion, and the piece map remain based exclusively
  on verified bytes. Live telemetry is labeled `RECEIVING`/`RECEIVING SPEED`
  so received bytes are never represented as verified download completion.

## 1.0.35+36

- Bumped the coordinated Flutter release to `1.0.35+36` and the Rust backend
  to `0.1.33`.
- Upgraded the matched Flutter/Rust Bridge runtime and generator from `2.12.0`
  to `2.13.0`, then regenerated the tracked bindings. The remaining newer
  package notices are SDK or direct-parent constraints: Flutter `3.47.1` pins
  its tooling transitive versions, while the latest `qr_flutter` still uses
  `qr` `3.x`; the AGP-9 compatibility overrides already select the latest
  `package_info_plus` and `wakelock_plus` releases.

## 1.0.34+35

- Bumped the coordinated Flutter release to `1.0.34+35` and the Rust backend
  to `0.1.32`.
- Added an Android Files picker backed by Storage Access Framework document
  URIs. Read permission is persisted and the Rust torrent storage opens the
  original provider descriptor for each range read; selected media is never
  copied into a Portalis cache or staged as a duplicate source.
- Retried failed durable torrent metadata resolves with a bounded backoff, so
  a temporarily unavailable magnet no longer remains indefinitely at
  `Import torrent` until the app restarts.

## 1.0.32+33

- Bumped the coordinated Flutter release to `1.0.32+33` and the Rust backend
  to `0.1.30`.
- Made FRB generation incremental: bridge bindings regenerate only when the
  explicit bridge inputs or generated outputs change, with `--force-frb` and
  `--no-frb` escape hatches.
- Integrated the conditional FRB check into the canonical build/run tools and
  aligned the Windows generator/build scripts with the current bridge inputs.
- Added native build inputs/outputs and timestamp checks for macOS, iOS, and
  Android so unchanged Rust backends are not rebuilt or repackaged.

## 1.0.31+32

- Bumped the coordinated Flutter release to `1.0.31+32` and the Rust backend
  to `0.1.29`.
- Added a startup backend trust report showing the frontend version, loaded
  backend version, expected backend version, and whether compatibility was
  accepted or rejected. Compatibility failures now include all of those
  details instead of reporting only a generic version mismatch.

## Unreleased

### Added

- On iPhone, Portalis now automatically imports each hash-verified received
  photo or video into the system Photos library. The collection retains the
  corresponding `phasset://` reference for resume and seeding; unsupported
  files stay in Portalis storage, and the original app-local media is removed
  only after the Photos-backed storage rebind succeeds.

### Fixed

- Published owner collections now enter the explicit `Seeding` lifecycle
  instead of remaining at `Preparing` while librqbit checks the referenced
  source. The source-check cursor is shown as local verification progress, not
  as received download bytes; ready and active seeds identify their local
  source rather than displaying a misleading receiver progress total.

- Restored published zero-copy owner collections as seeding-ready for sharing
  while their seed rehydrates after restart. They no longer present the
  owner's original media as a receiver-side download before the engine's first
  live reading arrives.

- Kept incomplete transfer graphs anchored to their last real reading when
  receive activity stops, instead of adding synthetic zero-rate samples that
  stretch the timeline and compress earlier activity. The UI now retains the
  core's complete 30-minute, 500-ms history ring and removes the duplicate
  progress bar from the peer list.

- Fixed clean iOS builds deleting `backend.xcframework` and then invoking
  Xcode before its Rust producer ran. `tool/build.sh ios` now recreates the
  XCFramework before Flutter/Xcode plans the Runner target.

- Upgraded the Android build baseline to Gradle 9.1.0 and Android Gradle Plugin
  9.0.1 so Java 25 is supported by the local Flutter build, and migrated the
  app to AGP's built-in Kotlin mode. This also lets Kotlin-based plugins such
  as `file_picker` compile without the app applying a separate Kotlin Gradle
  Plugin and removes the legacy `kotlinOptions` block it no longer supports.
  The Android build remains opted out of AGP 9's new DSL while Flutter and the
  Rust task wiring use the legacy variant API. The FRB parser notices remain
  non-fatal diagnostics for internal, non-bridged Rust types. Updated
  `desktop_drop` to 0.8.0 for AGP 9 built-in Kotlin and explicit compile-SDK
  support. Overrode legacy transitive `package_info_plus` and `wakelock_plus`
  releases with their AGP 9-compatible implementations. Updated `file_picker`
  to 12.0.0 so the Windows FFI implementation natively supports `win32` 6.
- Fixed `tool/build.sh android` invoking the nonexistent `flutter build android`
  command. Android builds now invoke `flutter build apk`, matching the CI
  artifact pipeline.
- Removed the OpenSSL-backed `crypto-hash` path from the librqbit SHA wrapper
  used by native builds. The patched `sha1-ring` compatibility feature now uses
  portable Rust SHA-1/SHA-256 implementations, so Android cross-compilation no
  longer tries to build `openssl-sys` for `aarch64-linux-android`; Apple builds
  also avoid platform crypto objects.
- Made collection publication and torrent-import wakes lossless for arbitrary
  numbers of collections by using durable-state scans with `Notify` rather
  than a bounded channel whose `try_send` result was ignored. Referenced
  torrent metadata is now namespaced by info hash, so collections with the
  same display name cannot overwrite one another. Sender and receiver
  transitions now log the collection key and torrent identity for diagnosis.
- Kept the Share QR action visible while a published collection's list/detail
  projections rehydrate after restart. Publication recovery now keys off the
  durable substrate handle, allowing an interrupted collection with a revision
  but no handle to leave `Preparing` and publish again.
- Fixed subsequent zero-copy collections staying in `Preparing`: librqbit's
  persisted-ID allocator cannot see zero-copy torrents, so it reused the first
  live torrent ID and returned the first torrent as `AlreadyManaged`. Portalis
  now serializes managed torrent admission and allocates from the live session
  ID set for publishers, rehydration, downloads, and recovery.

- Broke the iOS Xcode dependency cycle between `ProcessXCFramework
  backend.xcframework` and the `Build Rust (iOS)` script phase. The Rust
  XCFramework is now produced by a dedicated `RustBackend` aggregate target
  that `Runner` depends on, so the framework exists before Xcode plans the
  Runner target instead of being written by a phase inside it. UPnP port
  forwarding, HTTPS, and the embedded/signed `backend.framework` topology are
  unchanged.

### Changed

- When the configured BitTorrent listen port is occupied, the backend now tries
  the remaining configured port range instead of leaving the entire torrent
  engine unavailable. The selected bound port is logged and remains the port
  advertised in shared QR peer hints.
- A stale zero-copy source during startup rehydration no longer tears down
  the live librqbit session or causes repeated listener creation attempts. The
  source error remains logged while the singleton session stays available for
  new collections and imports.


- Collection publication now uses one referenced-storage path for every
  source. Portalis reads and hashes the original files directly; it never
  creates hard-link, clone, or copied source layouts.

- Upgraded `mobile_scanner` to 7.4.0 for the current macOS AVFoundation camera
  selector and fallback behavior.

- Restored zero-copy gallery-backed torrents into the live librqbit session on
  startup from the persisted descriptor and original source references. QR
  sharing now continues to serve metadata after an iPhone or macOS restart
  without copying media.

- Added the macOS App Sandbox camera entitlement to both debug and release
  profiles.

- Added a cross-platform Share-menu `Scan QR code` action. It opens the camera,
  accepts Portalis collection QR links and legacy raw magnets, then imports the
  torrent and starts receiver-side selection/download without publishing local
  media.

- Kept native Android MediaStore and iOS PhotoKit adapters inside the Nexus
  platform namespace, separating platform glue from the app-facing Rust bridge
  and preserving a scalable backend layout.

### Removed

- Removed the Iroh-based Nexus control plane (QUIC transport, server, and client crates). The product is now BitTorrent-only; peer discovery uses the BitTorrent substrate with QR-first bootstrap (ADR-0003.5, ADR-0009). The collection crypto (capsule sealing, key rotation, chain verification) was extracted into `backend::crypto` and no longer depends on Iroh. The `portalis-nexus-client`, `portalis-nexus-server`, `portalis-nexus-server-core`, `portalis-nexus-storage`, `demo`, and `iroh` dependencies are gone.
- Removed the **Nexus service** section from Settings: with the Iroh-based service gone, there is no Node ID to report and no connectivity to observe. The Settings screen no longer loads or lists the service controller, and the service repository, endpoint config, and their controller tests are deleted.
- Removed the MongoDB storage backend (ADR-0002). The coordination node persists to the embedded redb engine only; PostgreSQL is the scale successor if a node ever saturates. `PORTALIS_NEXUS_MONGODB_URI` and `PORTALIS_NEXUS_DATABASE` are gone — set `PORTALIS_NEXUS_DATA_DIR` instead. `docker/compose.yaml` now runs the server against a named volume rather than a replica set, and the test suite no longer needs Docker.
- Removed obsolete `tool/run_app.sh` and `tool/nexus_server.sh` wrappers; use
  `tool/run.sh` for build-then-run workflows and the project deployment tooling
  for Nexus services.

### Fixed

- Fixed post-creation native collection additions: `AddMedia` now persists new
  original source references, refreshes the projection, and wakes publication
  instead of being silently accepted and discarded.

- Shared magnet imports now forward their embedded `x.pe` LAN peer hints during
  both metadata resolution and file acquisition, allowing the receiver to
  connect directly to the publishing Mac.

- Gallery-linked publication now remains zero-copy with librqbit 9.0.1 session
  persistence: referenced storage factories are explicitly skipped by the
  filesystem-only session serializer while Portalis keeps the torrent metadata
  and original source descriptors in its linked-source vault.

- Added `tool/build.sh` and `tool/run.sh` helpers for platform-aware Flutter
  builds and build-then-run workflows. Both support `--clean`; `--ai` keeps
  successful output concise while replaying full Cargo/Flutter output on errors,
  and both work without optional Flutter arguments under Bash strict mode.

- Kept UPnP and HTTPS enabled on iOS while replacing librqbit 9's AWS-LC Rustls
  provider with Rustls `ring` and the portable SHA-1 implementation. Portalis
  installs that provider before opening a torrent session, removing the
  AWS-LC iOS linker objects that referenced `___chkstk_darwin` and preventing
  reqwest's missing-provider panic from leaving a DHT socket bound.

- Aligned Rust, native dependency, and generated XCFramework deployment targets
  with Xcode's `IPHONEOS_DEPLOYMENT_TARGET` (15.0 by default). This prevents
  current-SDK native objects from being linked as iOS 10.0 binaries, which
  caused librqbit 9's AWS-LC dependency to fail with `___chkstk_darwin`.

- Patched librqbit's dual-stack socket helper for iOS, tvOS, and visionOS.
  Apple mobile targets now report device binding as unsupported instead of
  compiling the Linux-only `socket2::Socket::bind_device` call, allowing the
  iOS Rust build to proceed while preserving macOS and Linux interface binding.

- Upgraded the vendored BitTorrent engine from librqbit 8.1.1 to 9.0.1,
  retaining Portalis's verified piece-activity telemetry and adapting session,
  listener, metainfo, and storage APIs to the new engine. librqbit 9 no longer
  exposes the previous deferred-write-buffer option, so the existing deferred
  writes setting is preserved for compatibility but is not applied by the
  upstream engine.

- Magnet imports now honor a validated HTTPS/HTTP `.torrent` exact-source
  (`xs`) fallback when supplied. Portalis verifies the descriptor's BTv1 info
  hash against the magnet before accepting it, so magnets can resolve metadata
  even when DHT, tracker, and direct-peer discovery find no peer.

- Collection QR magnets now include the sender's private-LAN address and the
  BitTorrent session's actual bound port as direct `x.pe` bootstrap hints. A
  receiver on the same network can resolve the file list without relying on a
  tracker, DHT, or guessed default port. Torrent views now label their
  process-local collection number as **Local handle**, not as an info hash.

- Scanning a collection QR now stays on the receiver path: Portalis imports
  the Mac's magnet, waits for its file list, and starts downloading the
  selected remote files. Incoming torrents no longer present or invoke a
  local **Share this collection** action.

- **Share this collection** now starts torrent publication and seeding instead
  of leaving the collection paused after confirmation. Once publication
  persists its info hash, the collection exposes a usable share URI and QR.

- Fixed referenced collection publication persisting its torrent descriptor
  before librqbit opens referenced storage, allowing filesystem and iOS Photos
  shares to create and seed their torrent instead of failing with a missing
  descriptor.

- Fixed the staged Nexus migration opening `portalis.redb` twice on app start. The active legacy collection path and Nexus runtime now share one process-owned database handle until the legacy path is removed.

- Restored the headless Nexus demo's explicit device fingerprint configuration, so the complete Rust workspace builds and tests after the Nexus state projection began exposing device fingerprints.

- Nexus signatures now bind to the authenticated QUIC Node ID rather than a configurable host and port. `PORTALIS_NEXUS_SERVER_AUTHORITY` has been removed: direct addresses, relays, and DNS names are routing hints, while the stable Node ID is the server identity.

- Removed the obsolete Nexus WebSocket endpoint and client dependencies. The service now has one authenticated QUIC transport, while its HTTP surface is limited to liveness and readiness checks.

- Completed the Nexus client QUIC migration: connection and fault tests now exercise real Iroh peers, reconnecting clients release failed in-flight registrations, and swarm leases use only Iroh-verified direct UDP source addresses rather than internal transport addresses.

### Added

<!-- New user-visible features go here before the next release. -->

- Completed collections can now show a **Share QR** code. The QR contains a
  `portalis://import` deep link carrying the collection's validated BitTorrent
  magnet URI, so iOS and Android can open Portalis and enter the existing
  import flow; drafts and unresolved collections expose no QR code.

- **ADR-0003.5 / ADR-0009**: Peer hint parsing and QR-first bootstrap for offline sharing. Magnet URIs can now carry `x.pe` parameters with LAN peer addresses for direct device-to-device transfers. QR code scanner extracts magnet links with peer hints. LAN/mDNS discovery returns local network peers on the standard BitTorrent port (6881). FRB API exposed: `peer_hints_create`, `peer_hints_from_magnet`, `peer_hints_validate_address`, `peer_hints_discover_local`.

- Restored Home's prominent **Share files** action and the complete existing New Share flow on top of Nexus. Selected native files retain their names, measured sizes, and stable locations inside the collection aggregate; Nexus hashes and seeds them through the existing zero-copy substrate, persists the initial signed descriptor/revision, resumes preparation after restart, and streams honest file totals and detail without legacy polling.

- Nexus collection detail now supports renaming and safe collection-only deletion through the same command boundary as the Home library.

- Home now renders its collection library directly from the app-owned Nexus state stream. Its cards, filters, summaries, and collection routes no longer project or observe legacy collection records.

- Dropping or choosing a local `.torrent` now enters Nexus directly and opens its preparation screen. The file list is streamed from the resolved descriptor, can be narrowed before confirmation, and requests no payload bytes while the Nexus torrent substrate is still being connected.

- Imported `.torrent` files now retain their per-file selection in Nexus. Every file starts selected; confirming a narrower selection persists it across restart, while an empty selection is rejected before any download.

- Transfer panels now make download progress, byte totals, speeds, and ETA easier to scan. Peer sections show the collection's real local transfer progress without inventing a split per anonymous swarm address. The People screen replaces individual removals with a visible "Forget all remembered peers" action, undoable from its toast or `Ctrl+Z` (`⌘Z` on macOS) for six seconds.

- Local `.torrent` imports now resolve their descriptor and file metadata before a collection is shown. The persisted preparation view exposes every file and its size for later selection, while deliberately fetching no payload bytes.

- Torrent imports now start as durable owner-controlled Nexus collections in a preparation state. Portalis records a magnet URI or local `.torrent` source without downloading payload bytes, ready for metadata selection in the next workflow step.

- Portalis now starts one app-owned Nexus runtime and state subscription after verifying native/frontend compatibility. Backgrounding pauses its network work and shutdown drains it; generated bridge imports for this path are contained in one Dart adapter.

- Nexus collections now have a durable local starting point: creating, renaming, and deleting one updates the streamed app state immediately and persists in `portalis.redb` across restarts. The initial Nexus state now includes this device's fingerprint for later contact verification.

- Added the first app-facing Nexus bridge. It starts and stops one local runtime, streams complete state and on-demand collection detail, and accepts validated collection, people, lifecycle, and torrent-import commands. The new generated API uses its own stable DTOs while the old path remains temporarily available for the staged screen migration.

- Nexus is now ready to operate as Portalis's control-plane service: it serves authenticated QUIC on UDP and liveness/readiness probes on TCP at the same configured address and port. The service logs the stable Node ID users put# Changelog

## Unreleased

### Removed

- Removed the MongoDB storage backend (ADR-0002). The coordination node
  persists to the embedded redb engine only; PostgreSQL is the scale successor
  if a node ever saturates. `PORTALIS_NEXUS_MONGODB_URI` and
  `PORTALIS_NEXUS_DATABASE` are gone — set `PORTALIS_NEXUS_DATA_DIR` instead.
  `docker/compose.yaml` now runs the server against a named volume rather than
  a replica set, and the test suite no longer needs Docker.

### Fixed

- Fixed the staged Nexus migration opening `portalis.redb` twice on app start.
  The active legacy collection path and Nexus runtime now share one
  process-owned database handle until the legacy path is removed.

- Restored the headless Nexus demo's explicit device fingerprint configuration,
  so the complete Rust workspace builds and tests after the Nexus state
  projection began exposing device fingerprints.

- Nexus signatures now bind to the authenticated QUIC Node ID rather than a
  configurable host and port. `PORTALIS_NEXUS_SERVER_AUTHORITY` has been
  removed: direct addresses, relays, and DNS names are routing hints, while
  the stable Node ID is the server identity.

- Removed the obsolete Nexus WebSocket endpoint and client dependencies. The
  service now has one authenticated QUIC transport, while its HTTP surface is
  limited to liveness and readiness checks.

- Completed the Nexus client QUIC migration: connection and fault tests now
  exercise real Iroh peers, reconnecting clients release failed in-flight
  registrations, and swarm leases use only Iroh-verified direct UDP source
  addresses rather than internal transport addresses.

### Added

<!-- New user-visible features go here before the next release. -->

- Restored Home's prominent **Share files** action and the complete existing
  New Share flow on top of Nexus. Selected native files retain their names,
  measured sizes, and stable locations inside the collection aggregate;
  Nexus hashes and seeds them through the existing zero-copy substrate,
  persists the initial signed descriptor/revision, resumes preparation after
  restart, and streams honest file totals and detail without legacy polling.

- Nexus collection detail now supports renaming and safe collection-only
  deletion through the same command boundary as the Home library.

- Home now renders its collection library directly from the app-owned Nexus
  state stream. Its cards, filters, summaries, and collection routes no
  longer project or observe legacy collection records.

- Dropping or choosing a local `.torrent` now enters Nexus directly and opens
  its preparation screen. The file list is streamed from the resolved
  descriptor, can be narrowed before confirmation, and requests no payload
  bytes while the Nexus torrent substrate is still being connected.

- Imported `.torrent` files now retain their per-file selection in Nexus.
  Every file starts selected; confirming a narrower selection persists it
  across restart, while an empty selection is rejected before any download.

- Transfer panels now make download progress, byte totals, speeds, and ETA
  easier to scan. Peer sections show the collection's real local transfer
  progress without inventing a split per anonymous swarm address. The People
  screen replaces individual removals with a visible “Forget all remembered
  peers” action, undoable from its toast or `Ctrl+Z` (`⌘Z` on macOS) for six
  seconds.

- Local `.torrent` imports now resolve their descriptor and file metadata
  before a collection is shown. The persisted preparation view exposes every
  file and its size for later selection, while deliberately fetching no
  payload bytes.

- Torrent imports now start as durable owner-controlled Nexus collections in
  a preparation state. Portalis records a magnet URI or local `.torrent`
  source without downloading payload bytes, ready for metadata selection in
  the next workflow step.

- Portalis now starts one app-owned Nexus runtime and state subscription after
  verifying native/frontend compatibility. Backgrounding pauses its network
  work and shutdown drains it; generated bridge imports for this path are
  contained in one Dart adapter.

- Nexus collections now have a durable local starting point: creating,
  renaming, and deleting one updates the streamed app state immediately and
  persists in `portalis.redb` across restarts. The initial Nexus state now
  includes this device's fingerprint for later contact verification.

- Added the first app-facing Nexus bridge. It starts and stops one local
  runtime, streams complete state and on-demand collection detail, and accepts
  validated collection, people, lifecycle, and torrent-import commands. The
  new generated API uses its own stable DTOs while the old path remains
  temporarily available for the staged screen migration.

- Nexus is now ready to operate as Portalis's control-plane service: it serves
  authenticated QUIC on UDP and liveness/readiness probes on TCP at the same
  configured address and port. The service logs the stable Node ID users put
  into Portalis, drains both listeners on shutdown, and ships with a durable
  embedded-storage deployment guide.

- Added Nexus service setup to Portalis Settings. A person can save the
  server's public QUIC Node ID and a direct address; the native backend
  validates both as one trusted endpoint before any future connection can use
  them. The screen distinguishes a saved endpoint from a live connection.

- The Nexus service now has a stable authenticated QUIC identity. Embedded
  deployments generate and retain a private node secret beside their data;
  container and MongoDB deployments provide the same 32-byte secret through
  `PORTALIS_NEXUS_NODE_SECRET`. The derived public node ID is logged for
  clients to authenticate when they connect over QUIC.

- Added the first unified Nexus connection primitive. A `NexusEndpoint`
  reuses the app's existing Ed25519 device secret, returns raw authenticated
  QUIC connections and streams to its caller, and reports whether a peer path
  is direct, relayed, mixed, or unavailable. Direct/relay connectivity, TLS,
  framing, flow control, and NAT traversal come from Iroh rather than a custom
  Nexus channel or codec layer.

- Added truthful per-media torrent piece activity. Verified byte ranges now
  appear at their real relative positions around media previews, while live
  worker markers appear only for peers the engine has actually assigned to
  in-flight pieces; sequential downloads remain sequential instead of being
  visually spread across unrelated files.

- Completed Nexus M4 encrypted shares and snapshots. Private share discovery,
  fetches, live events, key delivery, and transient handoffs now require
  membership; encrypted capsules are bounded and immutable; publication uses
  transactional snapshot history and compare-and-swap heads so revisions
  cannot regress. The portable client exposes the complete flow.

- Rewrote the specification as Portalis v3. It described a control plane and
  narrated how that plane was built; a quarter of it was milestone history
  already recorded here. It now describes one product across four surfaces —
  the Flutter interface, the bridge between it and the Rust core, the peer and
  service protocol, and the service itself — and says what the system is
  rather than how it arrived. Two boundaries that were never specified now
  are: the application state contract, where the core owns the truth and the
  interface renders a projection it derives nothing from, with payload tiered
  by how often it changes and whether it is on screen; and the structure of
  the Flutter application itself. Responsiveness became measurable product
  requirements rather than an aspiration. The product version is v3 and the
  wire protocol remains `portalis.protocol.v1`, which the document now states
  outright because conflating them was itself a source of confusion.

- Started Nexus M6 with the client-side foundation the Portalis app needs to
  publish a share: the canonical manifest, the `SnapshotId` taken over it, and
  the capsule that carries it. Both live in the portable client crate, so
  every platform shares one implementation — Nexus stores a capsule it cannot
  open and a content root it cannot recompute, and so cannot catch two clients
  that disagree about a byte. Sealing is deterministic, which is what makes a
  retried publication byte-identical, and a capsule is bound to its share,
  revision, and snapshot so it cannot be lifted onto another.

- Hardened the Nexus manifest foundation for M6: names must be NFC-normalized,
  capsule opens verify each entry signature, and capsule nonces include the
  snapshot root so concurrent candidate revisions cannot reuse a key/nonce
  pair.

- Started the M6 app integration seam: granting a share now returns every
  non-revoked recipient device and its encryption key, while live `.torrent`
  handoffs carry the info hash and route only to the exact device (reporting
  an offline recipient as a typed unavailable response). The backend now
  imports the portable Nexus client behind a private native seam, keeping
  protobuf transport details out of the Flutter bridge.

- Started the native Portalis M6 integration with durable device credentials.
  The backend now implements the portable Nexus signer over the existing
  Ed25519 identity and generates an independent X25519 key for sealed share
  keys. Existing `identity.json` files are upgraded atomically on first load,
  without changing their signing key, legacy collection identity, or Flutter
  bridge schema.

- Extended the Nexus walkthrough demo through the complete online A→B path:
  it consumes the returned recipient device, seals the descriptor, sends it
  live, and verifies that only the exact device receives and opens it.

- Added the versioned client-side torrent-handoff codec: each descriptor is
  encrypted with a fresh nonce and authenticated to its share, recipient
  device, and info hash; malformed, oversized, non-NFC, tampered, or replayed
  into another context handoffs are rejected.

- Added a separate two-process Nexus M6 demo. Its standalone in-memory server
  and concurrent-client executable exercise registration, pings, friendship,
  socket-derived presence, capsule publication, recipient-device grants,
  sealed key delivery, exact-device encrypted `.torrent` handoff, and private
  swarm discovery over real connections. The server logs each socket and safe
  command/response label while keeping opaque encrypted payloads out of logs.

- Added Nexus share-membership revocation, so an owner can remove someone
  from a share rather than only ever adding them. Nexus stops answering that
  user with summaries, capsules, envelopes, and handoffs; it cannot reach the
  share key they already hold, so excluding them means rotating the key and
  publishing the next revision sealed to the members who remain. Revoking is
  idempotent, only the owner may do it, and the owner cannot be removed from
  their own share.

- Completed Nexus M5 swarm discovery. Authenticated seeders announce bounded
  short-lived leases using the IP observed by the socket, lookups prefer
  compatible recent peers across diverse network prefixes, and disconnects
  or expiration remove stale endpoints. Client candidates from direct,
  Nexus, tracker, and DHT discovery now merge without duplicates.

- Added Nexus M2.5 encrypted share-key delivery between a user's approved
  devices. Nexus stores only per-device X25519 envelopes, never plaintext
  share keys or media metadata.

### Changed

<!-- Behaviour and UX changes go here before the next release. -->

- Collapsed the Flutter/Rust bridge onto a single API seam (ADR-0001): the app
  now drives the backend exclusively through `AppSnapshot` reads and `Command`
  writes. The parallel legacy torrent bridge surface — `torrent.dart` and its
  generated per-call FRB entry points — has been deleted, and the FRB bindings
  were regenerated against the reduced surface. No user-facing capability is
  removed; the same operations are expressed as commands.

- Nexus command connections now run over authenticated QUIC streams. The
  service accepts connections concurrently and drains those streams gracefully
  on shutdown; its ALPN identifier is owned by the shared protocol crate.

- Unified the native backend and Nexus into one Cargo workspace rooted at
  `portalis/rust/backend`. The Flutter library is now the Nexus application
  composition root; protocol, client, and server remain focused internal
  crates, while Torrent is an engine owned by the same lifecycle rather than
  a sibling backend joined through a compatibility binding.

- Standardized structured tracing across every Nexus executable and both
  WebSocket boundaries. Clients and servers now correlate safe operation and
  response labels with message IDs, report connection, retry, timeout, and
  shutdown lifecycle events, and share one exhaustive protocol-owned label
  mapping. Capsules, key envelopes, handoff ciphertext, challenges, and
  private metadata remain deliberately absent from logs; narrated demo steps
  remain on stdout as user-facing output.

- Moved every Nexus timestamp to nanoseconds since the Unix epoch, on the
  wire, in the domain, and in storage. `Envelope.sent_at_unix_ms` is now
  `timestamp_unix_ns`, and `server_time_unix_ms` and `last_seen_unix_ms`
  follow the same `*_unix_ns` naming. Field numbers are unchanged, so this
  breaks the protocol without breaking binary decoding: a client built
  against the old schema still reads the field and interprets nanoseconds as
  milliseconds. Nexus is not deployed, so no compatibility window applies.
  `UUIDv7` keeps milliseconds internally, since its 48-bit timestamp is
  defined in them and nanoseconds would wrap it every three days.

### Fixed

<!-- Bug fixes and regressions go here. -->

- Replaced the ambiguous transfer-history sparkline with a labeled speed
  chart: download and upload now state whether a value is current or a peak,
  the vertical scale has units, the session duration and endpoints are
  explicit, and completed transfers no longer present `0.0 MB/s` as though it
  explained the historical line. Expanded collection cards now let progress,
  peers, remaining time, and history sit directly in the collection without a
  nested panel or empty-chart frame. Completed histories label their final
  timestamp as `END`, including older records without an explicit completion
  time. Expanded collection identity, progress, current/peak speed, and actions
  now share one responsive header above the graph instead of occupying separate
  vertical sections. Its action dock uses compact icon controls for lifecycle
  and destructive commands plus short labeled collection actions, keeping the
  full command set on one line at desktop widths. The progress track now closes
  the transfer visualization beneath the graph instead of separating its
  header from the chart. Transfer plots now use a logarithmic vertical scale
  so simultaneous upload and download remain visible despite large speed
  differences, and either direction keeps the temporal graph on screen.
  Collection actions no longer expose `Forget` or a files-only deletion;
  one `Delete` confirmation now offers `Cancel`, `Delete`, and `Delete with
  files`, with both destructive choices removing the collection. Media
  previews now stay tiny on wide collection cards instead of stretching with
  the four-column layout.
- Shortened the empty Home welcome to `Send anything to anybody`; all welcome
  copy now lingers briefly, then softly fades and collapses, replaying whenever
  Home is entered again while the logo remains available.
- Peer observations now show only their compact age (`4s`, `2m`, `1h`); active
  torrent peers retain their live ember treatment while remembered peers that
  are no longer connected receive a quieter, stable identity color of their
  own.

- Fixed the Settings efficiency benchmark reporting `0 ms`. The run is
  sub-millisecond, and whole milliseconds rounded it away, which reads as a
  broken benchmark rather than a fast one. It now measures in nanoseconds
  like the rest of Portalis — `Stopwatch` ticks that finely, and it was
  `Duration` discarding the precision — and scales the label to the run. The
  card also shows the cost of one operation, and a run too fast for the clock
  no longer reports an infinite rate.

- Fixed the Nexus server ignoring `SIGTERM`, so every container restart killed
  it rather than stopping it. Only `SIGINT` was listened for, which no
  orchestrator sends: `docker compose stop` waited out its grace period and
  killed the process with exit 137, severing live sockets instead of draining
  them. It now stops on either signal, and a signal handler that cannot be
  installed waits rather than reporting an immediate shutdown.

- Fixed every Nexus WebSocket upgrade failing with HTTP 500 in the demo.
  Swarm discovery binds a peer lease to the address the socket observed, which
  needs the service to carry connect info; the demo served its router without
  it. The server process and the test harness already did, which is why the
  suite stayed green.

- Fixed a Nexus key-envelope write that could drop a rotated share key while
  reporting a storage outage: two devices pushing for the same share and
  recipient at once made the unique index reject the loser, which is a lost
  race to retry rather than a failure to report.

- Hardened Nexus key envelopes against low-order X25519 keys and record
  transplanting, and bounded retrieval with deterministic cursor pagination.

### Engineering

<!-- Tests, tooling, refactors, and maintenance notes go here. -->

- Defined Portalis as the composition of two sibling engines before extending
  M6 further. Nexus owns authenticated protobuf control and may hand a caller
  a separately authenticated binary WebSocket when arbitrary live traffic is
  actually needed; the caller owns its payload type and serialization. Nexus
  adds no codec registry, JSON/protobuf adapters, custom framing, or second
  encryption protocol. Torrent remains responsible for descriptors, pieces,
  seeding, and discovery policy; Manifest sharing coordinates the two engines.

- Specified everything the Portalis clients need before M6 can start. The
  capsule format and the canonical manifest encoding behind `SnapshotId` are
  now defined byte for byte: both were contracts every client must agree on
  that no client could have derived, and Nexus is structurally unable to
  catch a disagreement about either. Device identity is reconciled — the app
  adopts the derived identifier and manifest entries carry the public key —
  along with where the X25519 key lives and what losing it costs. Private
  shares get a stated threat model: v1 rests on info-hash secrecy and
  requires private torrents kept off DHT, PEX, and local discovery, which is
  weaker than encrypting payloads and now says so. Rate limits and per-user
  quotas have numbers, blocking has rules, and a new M5.5 milestone adds the
  commands a client cannot build without: removing a share member, listing
  and revoking devices, blocking, changing a handle, and deleting an account.

- Extended the Nexus demo walkthrough through M4 and M5, so both milestones
  are exercised by running them rather than only by their tests: an encrypted
  share published and advanced, a stale revision refused, a private share and
  a nonexistent one answering identically, access granted, and the share key
  sealed to a second user's device and opened there. `DemoDevice` now carries
  a real X25519 keypair beside its signing key — the placeholder it held
  before could not have sealed anything — and persists both. Two seeders then
  announce, discover each other at their observed addresses, and withdraw.

- Closed the Nexus M4 and M5 coverage gap. Both new handlers shipped without
  test modules, so the share and swarm commands are now covered at the socket
  boundary: every command refusing an unauthenticated connection, malformed
  identifiers, private shares answering the same way whether or not they
  exist, members hearing a publication, handoffs reaching only devices that
  may already read the share, and each domain refusal reaching the wire as a
  typed code. Also covers the publication retry loop — losing a
  compare-and-set to an identical publication, and exhausting the retries —
  the in-memory store's refusal to move a head that moved or rewrite a
  published revision, the swarm's one-peer-per-network pass filling a
  response on its own, and the storage failures inside `fetch` and `grant`.

- Gated Nexus line coverage on the merged profile instead of the summary
  percentage. The summary counts a line once per generic instantiation, and a
  service reached through several stores cannot run every line from each of
  them: the production store never loses a compare-and-set and a
  fault-injecting double never completes a write, so both paths are covered
  while no single instantiation covers both. The gate now fails on an
  uncovered line and names it, which is both stricter — no percentage of
  slack — and what "100% line coverage" was always meant to assert.

- Removed an unreachable `unreachable!` from share publication by deciding the
  write's precondition and the identical-retry answer in one match, so the
  fourth case no longer exists to be impossible.

- Closed the Nexus M2.5 coverage gap, restoring the 100% line and function
  gate. Covers the durable key-envelope store against a real replica set
  (rotation replacing its predecessor, listings scoped to the device they
  name, and the page boundary where MongoDB's ordering of binary values has
  to agree with the page type), the key-envelope handler, the client's
  envelope builders and page parsing, and opening an envelope that names a
  low-order ephemeral key.

- Serialized Docker-backed Nexus MongoDB integration tests and bounded every
  Docker readiness, connection, and shutdown wait so an unhealthy daemon
  reports a clear failure instead of hanging the suite.

- Started Portalis Nexus as an isolated Rust workspace for the new protobuf
  control-plane protocol and Linux discovery server, with portable client and
  server-core crates, health endpoints, container packaging, and dedicated CI.
- Fixed the Nexus coverage gate to use stable `cargo-llvm-cov` line, function,
  and region thresholds instead of unsupported branch-threshold flags.
- Added the first Nexus WebSocket transport slice: bounded protobuf frames,
  subprotocol negotiation, validated server hello, and correlated ping/pong
  with a real client/server integration test.
- Added bounded exponential reconnect with randomized jitter to the portable
  Nexus client, verified by two clients reconnecting after a forced restart.
- Gave each Nexus socket a bounded outbound queue and a dedicated writer task,
  so a peer that stops reading is disconnected instead of growing server memory.
- Added graceful Nexus socket draining, so shutdown signals every live
  connection and waits for it to close within a bounded timeout.
- Completed the Nexus connection lifecycle: the client is now a supervised
  handle that correlates concurrent requests, times them out, reconnects on its
  own, and exposes server-initiated envelopes as an event stream.
- Split the Nexus crates into focused modules, each covered by its own tests.
- Added the Nexus identity contract: registration and device-authentication
  messages, domain-separated signing payloads that bind a signature to one
  operation, connection, server, and challenge, BLAKE3 device identifiers
  derived from existing Ed25519 keys, and user handle rules.
- Added the Nexus registration and device-authentication rules over injected
  storage, time, and randomness: single-use expiring challenges, handle
  allocation that retries random discriminators against the unique index, and
  device revocation, writing each user and its first device atomically.
- Wired Nexus registration and device authentication end to end over the socket,
  with per-connection challenge state, typed refusal codes, and client commands
  that sign through a caller-supplied device signer. Storage remains in-memory
  until the durable adapter lands.
- Added a Nexus demo: a narrated walkthrough of the whole identity flow and a
  two-process client that persists its device key. Running it found a bug where
  a connection stamped its hello and its challenge from two separate clock
  readings, so every signature failed whenever the two straddled a millisecond.
- Added the Nexus friendship contract and state machine: one canonical edge per
  pair, idempotent commands, and versioned transitions for deterministic
  concurrent accepts, rejects, and removals, plus the friend service over it:
  handle resolution, friend listing, and commands that re-read and re-apply
  when another side writes first.
- Separated the Nexus server's layers: the socket moves bytes, the session
  holds who a connection is, and a handler module per subsystem decides what a
  command means. Adding a subsystem no longer changes transport code, and the
  protobuf build discovers schemas instead of listing them.
- Added Nexus presence aggregation across a user's devices, and served handle
  resolution, friend commands, and friend listing over the socket, verified by
  two clients becoming friends end to end.
- Completed Nexus friends and presence: events reach accepted friends only, a
  connection learns where its friends stand on arrival, and only the last
  device leaving reports a user away.
- Added transport integration suites for connection, reconnect, and event
  behaviour, which caught three client defects: a shutdown requested before the
  supervisor first ran was never observed, tearing down a peer-closed socket
  panicked the supervisor and stranded in-flight requests, and a handshake
  against a peer that never greeted could stall forever.
- Added durable MongoDB-backed Nexus identity and friendship storage with
  transactional registration, unique indexes, optimistic friendship writes,
  and real replica-set integration coverage. The server now refuses to start
  without a configured and reachable MongoDB instance.

## Portalis 1.0.8 / backend 0.1.6 — 2026-08-07

### Added

<!-- New user-visible features go here before the next release. -->

- Added native, bounded HEIC/HEIF preview decoding for iOS, macOS, and
  Android without changing or duplicating the original media file.

### Changed

<!-- Behaviour and UX changes go here before the next release. -->

- Moved every Nexus timestamp to nanoseconds since the Unix epoch, on the
  wire, in the domain, and in storage. `Envelope.sent_at_unix_ms` is now
  `timestamp_unix_ns`, and `server_time_unix_ms` and `last_seen_unix_ms`
  follow the same `*_unix_ns` naming. Field numbers are unchanged, so this
  breaks the protocol without breaking binary decoding: a client built
  against the old schema still reads the field and interprets nanoseconds as
  milliseconds. Nexus is not deployed, so no compatibility window applies.
  `UUIDv7` keeps milliseconds internally, since its 48-bit timestamp is
  defined in them and nanoseconds would wrap it every three days.

- Restyled shared collection rows with the same visual energy as torrent rows,
  including an accented preview tile and a clear shared-state fallback.

## Portalis 1.0.7 / backend 0.1.6 — 2026-08-07

### Added

<!-- New user-visible features go here before the next release. -->

- Added a Future theme — an electric cyan/violet palette drawn from the new
  app icon — as an alternative to the original mint-green Nature theme.
  Switch between them from Settings → Appearance; the choice persists across
  restarts.
- Added iOS Files open-in-place sharing: selected sources retain security-
  scoped, read-only access across restarts while Rust seeds the original file.
- Added no-copy iOS Photos & Videos selection backed by Rust-owned linked
  gallery sources.

### Changed

<!-- Behaviour and UX changes go here before the next release. -->

- Moved every Nexus timestamp to nanoseconds since the Unix epoch, on the
  wire, in the domain, and in storage. `Envelope.sent_at_unix_ms` is now
  `timestamp_unix_ns`, and `server_time_unix_ms` and `last_seen_unix_ms`
  follow the same `*_unix_ns` naming. Field numbers are unchanged, so this
  breaks the protocol without breaking binary decoding: a client built
  against the old schema still reads the field and interprets nanoseconds as
  milliseconds. Nexus is not deployed, so no compatibility window applies.
  `UUIDv7` keeps milliseconds internally, since its 48-bit timestamp is
  defined in them and nanoseconds would wrap it every three days.

- Removed the misleading iOS Photos import promise. iOS now offers Files as
  the no-copy source and never stages picker content in the app cache.
- Made collection synchronization automatically request and retry BitTorrent
  fetches when new manifest entries arrive, using direct peer hints when
  available.

### Fixed

<!-- Bug fixes and regressions go here. -->

- Silenced repetitive "peer stats unavailable" logging for torrents that
  simply aren't live yet (checking, queued, or peerless) — it was firing on
  every 500ms poll instead of only when a live torrent's peer stats
  genuinely failed.
- Silenced repetitive background collection, sync-address, and identity
  restoration logs.
- Fixed near-complete transfers displaying 100% before their final bytes, and
  stopped anonymous peer cards from repeating collection-level speed and size
  as per-peer data.
- Reduced live transfer lag by coalescing overlapping refreshes and moving
  history persistence off the snapshot update path.
- Kept the internal hard-link layout for multi-file shares in Portalis' private
  state directory so source files no longer appear as copied files in Downloads.

### Engineering

<!-- Tests, tooling, refactors, and maintenance notes go here. -->

- Added CocoaPods integration for the media plugins on iOS and macOS, and
  updated the iOS Flutter bridge and Files picker for current Flutter/Xcode
  APIs so unsigned release builds compile on both Apple platforms.
- Added an opt-in two-process integration test that verifies encoded invites,
  manifest synchronization, automatic fetching, torrent hashes, and file
  contents.

## Portalis 1.0.6 / backend 0.1.5 — 2026-08-06

### Added

- Added pooled collaborator transfer summaries to People cards, including
  current rate, shared collection count, and total shared bytes.

### Changed

- Removed the redundant empty-search action and centered Share files button;
  the Portalis logo is now the single share affordance and remains below the
  collection content.
- Made filesystem publication one-copy: a single file seeds from its original
  path, while multi-file layouts use hard links and refuse a hidden copy when
  linking is unavailable.
- Stopped automatic mobile gallery imports because Photos/MediaStore imports
  create a duplicate while the original must remain available for seeding.
- Temporarily refuse mobile path-picker sharing and add-media flows: picker
  cache paths cannot satisfy Portalis' no-copy contract before the native URI
  storage adapter exists.

### Fixed

- Prevented a hidden fallback copy when a desktop multi-file share cannot use
  hard links, keeping the one-copy storage guarantee intact.

### Engineering

- Documented canonical media storage, including the Android MediaStore adapter
  boundary and the one-copy iOS Files-library policy.
- Introduced Rust's canonical content-location boundary, which rejects an
  unsupported `content://` URI instead of converting it into a copied cache
  file.
- Added the Android application-context bootstrap required for Rust-owned
  MediaStore storage; it transfers no media bytes through Flutter.

## Portalis 1.0.5 / backend 0.1.4 — 2026-08-06

### Added

- Added live native import progress for collection creation and media batches,
  covering bounded copying, torrent hashing, seed startup, and visible failures.
- Added durable Rust import descriptors so an interrupted large share resumes
  from the same staging batch after Portalis restarts instead of becoming an
  empty collection with lost progress state.

### Changed

- Unified compact and wide window navigation under one adaptive shell state,
  preserving open collections and add flows while resizing on Windows.
- Tightened collection previews around a fuller first media row, grouped
  Invite and Add media with lifecycle actions, removed manual Sync from the
  collection view, and added a compact media-details layout with refresh.
- Moved collection publication to Rust-owned native file paths: Flutter now
  sends lightweight descriptors while Rust validates, copies, hashes, and
  seeds files without loading their contents into Dart or the FFI bridge.
- Made each published batch an immutable Portalis-owned snapshot in the
  configured download folder, preventing later source edits or deletion from
  corrupting pieces already announced to the swarm.
- Preserved selected media byte-for-byte, including HEIC originals, instead of
  rewriting files in the frontend before sharing.

### Fixed

- Fixed narrow mobile collection peer chips overflowing and prevented the
  Settings efficiency benchmark from leaving pending test timers.
- Removed the one-shot byte payload that could overflow the native bridge when
  creating a large share; share size is now bounded only by filesystem,
  platform, and BitTorrent implementation constraints.
- Made atomic state replacement work when the destination JSON already exists
  on Windows, including repeated import-progress persistence.

### Engineering

- Removed the frontend HEIC conversion dependency and obsolete payload-limit
  tests now that ingestion has one Rust-owned path.
- Regenerated Flutter-Rust Bridge bindings around `SourceFile` and native
  `ImportInfo`, and made the Windows helper discover Cargo-installed codegen
  from Git Bash.
- Added coverage for large persisted import descriptors, portable source-name
  validation, active-versus-failed polling, atomic state replacement, and the
  unified responsive navigation contract.

## Portalis 1.0.4 / backend 0.1.3 — 2026-08-06

### Added

- Added real collection lifecycle controls for restart, pause, stop, remove,
  and deleting local files with confirmation.
- Added a shared live transfer panel with prominent progress, rates, peers,
  ETA, and download/upload activity graph.
- Added persistent anonymous torrent-peer history with last-seen timestamps
  and explicit forget actions in People and collection details.
- Added persistent transfer history so collection download graphs and
  completion dates survive application restarts.
- Added first-frame video thumbnails and unified cross-platform video playback
  in the media viewer.
- Added a dedicated User destination for device identity and a local efficiency
  benchmark that refreshes whenever Settings is entered.

### Changed

- Made Stop non-destructive: it now halts transfer activity while retaining the
  collection and torrent for later restart.
- Exposed the complete collection command strip directly on every collection
  card; desktop expansion now focuses on the detailed preview without repeating
  those controls.
- Removed duplicate titles from expanded collection cards, compacted their
  command buttons, and increased spacing around filters and collection rows.
- Reworked transfer previews around polling-backed download/upload history from
  transfer start to current or completion, with date-time labels and a lighter
  energy treatment.
- Replaced the separate New share and Add torrent controls with one command
  input; Enter on an empty input opens the add flow and `.torrent` selection is
  restricted to torrent files.
- Unified collection detail presentation across mobile and desktop, removed the
  redundant information toggle, and replaced the desktop sidebar with a top
  identity and event rail.
- Moved transient notifications to the energized top event rail.
- Separated User and Settings across mobile and desktop navigation so identity
  stays distinct from engine configuration.
- Unified named collaborators and anonymous torrent peers into one collection
  view peer section, retaining last-seen details and forget actions.
- Enabled inline preview attempts for MKV and AVI movies, with external-player
  fallback when the platform codec is unavailable.
- Made media details visible by default and replaced the Windows-only playback
  path with a broader cross-platform codec backend.
- Expanded the media viewer preview to use most of the available viewport while
  keeping transfer facts and details accessible below it.
- Made the Portalis logo a persistent Home button in the desktop header.
- Aligned the Flutter-Rust Bridge toolchain and generated bindings to 2.12.0.

## Portalis 1.0.3 / backend 0.1.2 — 2026-08-05

### Changed

- Reorganized the Flutter frontend around explicit Collections and Media
  feature boundaries, with route-level screens and reusable presentation
  components colocated with their feature.
- Made Media independently reusable: file metadata, format capabilities, HEIC
  conversion, thumbnails, and the in-app viewer now live under
  `features/media`.
- Moved the remaining cross-feature visual primitives into `design/`; removed
  the transitional `ui/` implementation layer.

### Engineering

- Kept collection transfer, collaboration, and persistence state in the
  Collections feature while exposing `MediaPreview` as the extension point for
  future in-app image, text, audio, document, and richer video previews.
- Updated tests and imports to use feature-level public paths.

## Portalis 1.0.2 / backend 0.1.2 — 2026-08-05

### Fixed

- State files that cannot be read are now reported as errors instead of being
  mistaken for a first launch and overwritten.
- Settings accepts both normal and typographic dashes in the listen-port range.

### Changed

- Consolidated Flutter state into feature controllers, repositories, and pure
  domain models, with one application composition root.
- Confined Flutter-Rust Bridge DTOs to bootstrap and feature data adapters.
- Split Home and Settings into focused presentation components while keeping
  the same adaptive desktop and compact layouts.
- Renamed the backend sync scheduler around its actual responsibility:
  reconciliation.

### Engineering

- Added focused tests for listen-port parsing, settings copies, and vault read
  failures.
- Removed obsolete model, UI-barrel, and singleton-service compatibility code.

## Portalis 1.0.1 / backend 0.1.1 — 2026-08-05

### Added

- Added a persisted download-folder selector in Settings, supported on desktop
  and mobile.
- Added startup detection for a stale or mismatched native Rust backend.

### Fixed

- A Dart/Rust bridge mismatch now reports a clear compatibility error instead
  of failing later with a `RangeError` while decoding generated bindings.
- The startup compatibility message now gives the exact Windows release-build
  command needed when Flutter is loading an older native DLL.
- Download-folder changes are treated as restart-required and do not move
  torrents that were already registered.

### Engineering

- Added repository contribution instructions in `AGENT.md`.
- Corrected the Windows build helper to generate bindings from only the
  intended bridged modules.
