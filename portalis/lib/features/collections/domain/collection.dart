import '../../../nexus/domain/app_state.dart';
import '../../media/domain/item.dart';

/// One collection, as the interface reads it.
///
/// A view over the engine's own projection, not a copy of it. Every getter
/// below reads through to [source] or [detail]; nothing is stored, so nothing
/// can disagree with what the engine said.
///
/// It used to be a copy, built by a translation that restated every number and
/// rewrote every status word into a second vocabulary. Each restatement was a
/// place for two answers to drift, and all of them did: a bar measured the
/// selected files' progress against every file's size, a paused collection
/// reported itself as importing because one word had no mapping, and a list
/// row with no entries was handed a list of fabricated empty ones so the type
/// would be satisfied. None of those are possible to write here.
///
/// [detail] is absent for a row in a list, which has not asked for one — Nexus
/// charges nothing for a detail until something subscribes. Where it is
/// absent, [media] is empty rather than invented, and the counts come from the
/// snapshot, which knows them without it.
class Collection {
  const Collection(
    this.source, {
    this.detail,
    this.contacts = const [],
    this.lastReading,
  });

  final AppCollection source;
  final AppDetail? detail;
  final List<AppContact> contacts;

  /// The newest recorded reading, where something is accumulating them.
  final Reading? lastReading;

  /// The process-local handle. `AppCollection` carries no durable public
  /// identifier yet, and inventing one here would be worse than showing the
  /// handle the rest of the interface already uses.
  String get id => '${source.id}';
  String get name => source.name;

  /// The engine's own word, not a translation of it. Comparing against these
  /// directly is what keeps a status that nobody remembered to map from
  /// silently reading as every question's `false`.
  String get state => source.status;

  bool get isTorrent => source.nature == 'Torrent';
  bool get isShared => !isTorrent;

  /// Chosen but not shared: private to this device, and free to abandon.
  bool get isDraft => state == 'Draft';

  /// Told to stop. A person's decision, so it outranks whatever the numbers
  /// are doing — and it decides which half of the start/stop pair is offered.
  bool get isPaused => state == 'Paused';
  bool get isConnecting => state == 'WaitingForOwner';
  bool get isDownloading => state == 'Downloading';
  bool get isPreparing => state == 'Preparing';

  /// Complete, and with something to serve. Nothing to serve means nothing is
  /// being served — telling somebody their photos are available when the
  /// collection is empty is the kind of claim this interface must not make.
  bool get isSharing => isComplete && entryCount > 0;

  bool get isComplete => state == 'Available';
  bool get isMoving =>
      isDownloading || downBytesPerSecond > 0 || upBytesPerSecond > 0;

  int get totalBytes => source.totalBytes.toInt();
  int get downloadedBytes => source.onDiskBytes.toInt();
  int get uploadedBytes => source.uploadedBytes.toInt();
  int get revision => source.revision.toInt();

  /// Zero to one. The engine's own reading while a transfer is live, and the
  /// recorded history's last reading once it is not — never an arithmetic
  /// guess of this interface's own.
  double get progress =>
      source.transfer?.progress ?? lastReading?.progress ?? 0;

  /// Bytes per second, as the engine counts them. Never megabits: converting
  /// to them and then labelling the result "MB/s" showed every rate in the
  /// app at eight times its real value.
  int get downBytesPerSecond => source.transfer?.downBytesPerSecond ?? 0;
  int get upBytesPerSecond => source.transfer?.upBytesPerSecond ?? 0;
  int get livePeers => source.transfer?.peers ?? torrentPeers.length;
  int? get etaSecs => source.transfer?.etaSecs;

  /// `"ip:port"` of this collection's live swarm peers. Anonymous: BitTorrent
  /// carries no identity beyond a network address, so there is no name to
  /// show, only that somebody is there.
  List<String> get torrentPeers => detail?.peers ?? const [];

  /// The people this collection is shared with, resolved against the contacts
  /// the snapshot already carries.
  List<AppContact> get collaborators => [
        for (final member in source.members)
          for (final contact in contacts.where((item) => item.id == member))
            contact,
      ];

  /// How many entries there are, which the snapshot knows without a detail.
  int get entryCount => source.entries;

  /// Files, once something has asked for them. Empty rather than fabricated
  /// where nothing has: a row that did not subscribe does not have them, and
  /// saying so is cheaper than inventing placeholders that claim nothing.
  List<MediaItem> get media => [
        for (final entry in detail?.entries ?? const <AppEntry>[])
          mediaItemFor(entry),
      ];

  /// When bytes first moved, and when the engine reported it finished.
  ///
  /// The core's own record of each moment, not a measurement of whatever
  /// transfer history survived — that measured the ring rather than the
  /// transfer, and read a delete-and-re-add as one six-minute download when
  /// it had been two of half a minute.
  DateTime? get startedAt => _moment(source.startedAt);
  DateTime? get completedAt => _moment(source.completedAt);

  /// How long it took, once both moments exist.
  Duration? get completedIn {
    final from = startedAt;
    final to = completedAt;
    return from == null || to == null ? null : to.difference(from);
  }

  static DateTime? _moment(BigInt? unixNanoseconds) => unixNanoseconds == null
      ? null
      : DateTime.fromMicrosecondsSinceEpoch(
          (unixNanoseconds ~/ BigInt.from(1000)).toInt(),
        );

  /// Entries wanted but not yet here.
  int get pendingMedia => detail == null
      ? 0
      : detail!.entries.where((entry) => !entry.available).length;
}

/// One file, as the grid and the viewer read it.
///
/// Progress is the engine's own per-file byte count. Deriving it from
/// availability alone made every file binary: nothing at all until the very
/// end, then everything — for a ten-file torrent, ten empty tiles for the
/// whole download, which is precisely the question a person is asking while
/// they wait.
MediaItem mediaItemFor(AppEntry entry) {
  final total = entry.bytes.toInt();
  final done = entry.downloadedBytes.toInt();
  return MediaItem(
    entryId: entry.id,
    selected: entry.selected,
    label: entry.label,
    // Only once it is whole. A torrent's pieces arrive out of order, so a path
    // to a half-written file would render as a broken preview rather than as
    // the honest placeholder a person expects while waiting.
    localPath: entry.available ? entry.path : null,
    progress: total == 0 ? 0 : (done / total).clamp(0.0, 1.0),
    sizeBytes: total,
    downloadedBytes: done,
    // "Something of this is here", not "all of it is": a file that has begun
    // arriving reports how far along it is, rather than claiming nothing.
    fetched: entry.available || done > 0,
  );
}
