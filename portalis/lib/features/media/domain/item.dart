/// One file projected by the backend, independent of the collection that
/// carries it. Collection entries add grouping and collaboration context.
class MediaItem {
  const MediaItem({
    required this.label,
    String? entryLabel,
    this.localPath,
    this.progress = 0.0,
    this.sizeBytes = 0,
    this.downloadedBytes = 0,
    this.pieceRuns = const [],
    this.fetched = true,
    this.addedBy,
    this.entryId,
    this.selected = true,
  }) : _entryLabel = entryLabel;

  final String label;
  final String? _entryLabel;
  final String? localPath;
  final double progress;
  final int sizeBytes;
  final int downloadedBytes;
  final List<MediaPieceRun> pieceRuns;
  final bool fetched;
  final String? addedBy;

  /// How the backend addresses this file when it is asked to fetch or drop
  /// it. `null` where files are not individually addressable, which is what
  /// makes them unselectable rather than selectable-and-broken.
  final int? entryId;

  /// Whether this file is one the collection is set to fetch.
  ///
  /// Only meaningful where a source offers the choice at all; everywhere else
  /// every file is simply wanted, which is the default.
  final bool selected;

  /// The signed name of the collection entry that introduced this file.
  String get entryLabel => _entryLabel ?? label;

  /// The backend exposes a path only after it has verified completion, so the
  /// path is the readiness signal. This avoids a stale progress value hiding
  /// a valid local file.
  bool get isReady => localPath != null;

  /// Creates a presentation-only selection view without altering the source
  /// location, verified progress, or other durable media facts.
  MediaItem withSelected(bool value) => MediaItem(
        label: label,
        entryLabel: _entryLabel,
        localPath: localPath,
        progress: progress,
        sizeBytes: sizeBytes,
        downloadedBytes: downloadedBytes,
        pieceRuns: pieceRuns,
        fetched: fetched,
        addedBy: addedBy,
        entryId: entryId,
        selected: value,
      );
}

/// One exact file-relative intersection with torrent piece state. Missing
/// byte ranges are intentionally absent.
class MediaPieceRun {
  const MediaPieceRun({
    required this.offsetBytes,
    required this.lengthBytes,
    required this.verified,
    this.peers = const [],
  });

  final int offsetBytes;
  final int lengthBytes;
  final bool verified;
  final List<String> peers;

  bool get isDownloading => !verified && peers.isNotEmpty;
}
