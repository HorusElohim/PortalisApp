/// The engine's projection, as the app reads it.
///
/// The generated bridge types *are* the domain types. There used to be a
/// hand-written mirror of each one — same field names, same types, copied
/// across in an adapter — which cost about two hundred lines and paid for
/// nothing: a field added in Rust reached the generated class, was never
/// copied, and silently failed to exist for the interface. Silent omission is
/// the one failure this codebase cannot afford, so the copy is gone.
///
/// What the mirror was actually for lives here instead, as extensions and
/// derivations that codegen cannot express. Everything else is re-exported, so
/// the app still imports one stable path rather than reaching into a generated
/// file — the boundary the mirror claimed to draw, drawn for real this time.
library;

import 'dart:typed_data';

import '../../design/transfer_graph.dart' show TransferPoint;
import '../bridge/portalis_api.dart';

export '../bridge/portalis_api.dart'
    show
        AppAccepted,
        AppCollection,
        AppContact,
        AppDetail,
        AppDevice,
        AppEntry,
        AppPending,
        AppSnapshot,
        AppSourceFile,
        AppTransfer;

/// The history and piece map a detail carries, decoded.
///
/// An extension rather than a class, because the bytes come from the engine
/// and only their reading is the app's business. Codegen owns the fields; this
/// owns what they mean.
extension DetailReadings on AppDetail {
  /// The transfer history, decoded.
  ///
  /// Rows are `at_unix_ns(8) | down(4) | up(4) | progress_permille(2)`,
  /// big-endian, packed by the core. Decoded here rather than accumulated as
  /// the app observes readings: the core records whether or not a screen is
  /// open, so this history survives a restart and every screen sees the same
  /// one.
  ///
  /// Decoded into records rather than into any one widget's type, so a caller
  /// that needs the progress column and a caller that needs only the rates
  /// read the same bytes the same way.
  List<({DateTime at, double downloadMbps, double uploadMbps, double progress})>
      get readings {
    const rowBytes = 18;
    final rows = <
        ({
          DateTime at,
          double downloadMbps,
          double uploadMbps,
          double progress
        })>[];
    for (var offset = 0;
        offset + rowBytes <= samples.length;
        offset += rowBytes) {
      final bytes =
          Uint8List.fromList(samples.sublist(offset, offset + rowBytes));
      final row = ByteData.sublistView(bytes);
      rows.add((
        at: DateTime.fromMicrosecondsSinceEpoch(row.getUint64(0) ~/ 1000),
        downloadMbps: _megabits(row.getUint32(8)),
        uploadMbps: _megabits(row.getUint32(12)),
        progress: row.getUint16(16) / 1000,
      ));
    }
    return rows;
  }

  /// The history as the graph draws it.
  List<TransferPoint> get history => [
        for (final reading in readings)
          TransferPoint(
            at: reading.at,
            downloadMbps: reading.downloadMbps,
            uploadMbps: reading.uploadMbps,
          ),
      ];

  /// How far along the newest reading was, zero to one, or null when there is
  /// no history to say.
  double? get progress => readings.isEmpty ? null : readings.last.progress;

  /// Whether the bar at [index] is verified.
  bool pieceAt(int index) {
    final byte = index ~/ 8;
    if (byte < 0 || byte >= pieces.length) return false;
    return pieces[byte] & (1 << (index % 8)) != 0;
  }

  /// How many bars the piece map carries.
  int get pieceCount => pieces.length * 8;
}

extension EntryPreview on AppEntry {
  /// Whether this entry can be shown as a picture.
  bool get isImage {
    final name = label.toLowerCase();
    return const ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.heic', '.bmp']
        .any(name.endsWith);
  }
}

/// What the engine is doing right now, as one answer.
///
/// Derived in exactly one place and read everywhere, because the alternative
/// is what shipped before it: the shell chrome counted transfers from the
/// legacy collections controller while Home counted them from Nexus, and the
/// two disagreed — "1 ACTIVE TRANSFER" above a window reading "0
/// collections". A person cannot act on a status that contradicts the list
/// beside it, so there is one derivation and no widget may make its own.
class NexusActivity {
  const NexusActivity({
    required this.transfers,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.peers,
  });

  static const idle = NexusActivity(
    transfers: 0,
    downBytesPerSecond: 0,
    upBytesPerSecond: 0,
    peers: 0,
  );

  /// Collections currently moving bytes.
  final int transfers;
  final int downBytesPerSecond;
  final int upBytesPerSecond;

  /// Peers across every collection, which is what "connected" means to a
  /// person: they do not think per collection.
  final int peers;

  bool get isMoving => transfers > 0;

  /// Megabits per second, the unit the glow and the rate labels speak.
  double get downMbps => downBytesPerSecond * 8 / 1000000;
  double get upMbps => upBytesPerSecond * 8 / 1000000;
  double get rateMbps => downMbps + upMbps;
}

/// One command envelope, in the shape a caller wants to write.
///
/// The one part of the old adapter that was doing real work rather than
/// copying: `AppCommand` requires `files` and `entries` at every call site and
/// wants a `Uint32List` where callers hold a `List<int>`. Defaults and that
/// conversion live here, so asking to pause a collection stays one line.
class NexusCommand {
  const NexusCommand({
    required this.kind,
    this.name,
    this.files = const [],
    this.collection,
    this.label,
    this.deleteFiles,
    this.paused,
    this.entry,
    this.source,
    this.entries = const [],
    this.contact,
    this.handle,
    this.accept,
    this.device,
    this.active,
  });

  const NexusCommand.importTorrent(String source)
      : this(kind: 'importTorrent', source: source);

  final String kind;
  final String? name;
  final List<AppSourceFile> files;
  final int? collection;
  final String? label;
  final bool? deleteFiles;

  /// Required by `setPaused` and ignored otherwise. Optional here because one
  /// envelope carries every command; the core refuses a pause that does not
  /// say which way.
  final bool? paused;
  final int? entry;
  final String? source;
  final List<int> entries;
  final int? contact;
  final String? handle;
  final bool? accept;
  final int? device;
  final bool? active;

  AppCommand toBridge() => AppCommand(
        kind: kind,
        name: name,
        files: files,
        collection: collection,
        label: label,
        deleteFiles: deleteFiles,
        paused: paused,
        entry: entry,
        source: source,
        entries: Uint32List.fromList(entries),
        contact: contact,
        handle: handle,
        accept: accept,
        device: device,
        active: active,
      );
}

double _megabits(int bytesPerSecond) => bytesPerSecond * 8 / 1000000;
