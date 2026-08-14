/// App-owned Nexus projection types.
///
/// Widgets and controllers depend on these values, never directly on generated
/// Flutter-Rust Bridge classes. The adapter owns that conversion in one place.
library;

import 'dart:typed_data';

import '../../design/transfer_graph.dart' show TransferPoint;

class NexusAppState {
  const NexusAppState({
    required this.device,
    required this.connectivity,
    required this.contacts,
    required this.collections,
    required this.alerts,
  });

  final NexusDevice device;
  final String connectivity;
  final List<NexusContact> contacts;
  final List<NexusCollection> collections;
  final List<String> alerts;
}

class NexusDevice {
  const NexusDevice({
    required this.name,
    required this.handle,
    required this.fingerprint,
    required this.devices,
  });

  final String name;
  final String? handle;
  final String fingerprint;
  final int devices;
}

class NexusContact {
  const NexusContact({
    required this.id,
    required this.displayName,
    required this.handle,
    required this.fingerprint,
    required this.verified,
    required this.friendship,
    required this.reachable,
  });

  final int id;
  final String displayName;
  final String? handle;
  final String fingerprint;
  final bool verified;
  final String friendship;
  final String? reachable;
}

class NexusCollection {
  const NexusCollection({
    required this.id,
    required this.name,
    required this.nature,
    required this.role,
    required this.revision,
    required this.status,
    required this.members,
    required this.entries,
    required this.totalBytes,
    required this.onDiskBytes,
    required this.uploadedBytes,
    required this.transfer,
    required this.pending,
  });

  final int id;
  final String name;
  final String nature;
  final String role;
  final BigInt revision;
  final String status;
  final List<int> members;
  final int entries;
  final BigInt totalBytes;

  /// How much of it this device is holding. Carried in the snapshot rather
  /// than asked for, so a size never lags the list it sits in.
  final BigInt onDiskBytes;

  /// What this device has sent for it this session. The engine's own
  /// counter, so a restart begins it again.
  final BigInt uploadedBytes;
  final NexusTransfer? transfer;
  final NexusPending? pending;
}

class NexusSourceFile {
  const NexusSourceFile({
    required this.name,
    required this.path,
    required this.bytes,
  });

  final String name;
  final String path;
  final BigInt bytes;
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

class NexusTransfer {
  const NexusTransfer({
    required this.progress,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.peers,
    required this.etaSecs,
  });

  final double progress;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final int peers;
  final int? etaSecs;
}

class NexusPending {
  const NexusPending({required this.command, required this.queued});

  final BigInt command;
  final bool queued;
}

class NexusDetail {
  const NexusDetail({
    required this.id,
    required this.entries,
    required this.pieces,
    required this.samples,
    required this.peers,
  });

  final int id;
  final List<NexusEntry> entries;

  /// One bit per bar, packed, least significant bit first.
  final List<int> pieces;

  /// Fixed-width history rows. Decode with [history] rather than reading this.
  final List<int> samples;

  /// Swarm addresses. Not contacts: a swarm peer carries no signed identity,
  /// so it is shown as an address and never named as a person.
  final List<String> peers;

  /// The transfer history, decoded.
  ///
  /// Rows are `at_unix_ns(8) | down(4) | up(4) | progress_permille(2)`,
  /// big-endian, packed by the core. Decoded here rather than accumulated as
  /// the app observes readings: the core records whether or not a screen is
  /// open, so this history survives a restart and every screen sees the same
  /// one.
  ///
  /// Decoded once, into records rather than into any one widget's type, so a
  /// caller that needs the progress column and a caller that needs only the
  /// rates read the same bytes the same way.
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

  static double _megabits(int bytesPerSecond) => bytesPerSecond * 8 / 1000000;
}

class NexusEntry {
  const NexusEntry({
    required this.id,
    required this.label,
    required this.bytes,
    required this.selected,
    required this.available,
    this.path,
  });

  final int id;
  final String label;
  final BigInt bytes;
  final bool selected;
  final bool available;

  /// Where the bytes landed, once they have. Resolved by the core rather than
  /// guessed from a media directory, so a preview either works or is absent.
  final String? path;

  /// Whether this entry can be shown as a picture.
  bool get isImage {
    final name = label.toLowerCase();
    return const ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.heic', '.bmp']
        .any(name.endsWith);
  }
}

/// One command envelope. Command-specific fields remain optional so adding a
/// command does not make Dart duplicate the bridge's generated union code.
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

  final String kind;
  final String? name;
  final List<NexusSourceFile> files;
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

  const NexusCommand.importTorrent(String source)
      : this(kind: 'importTorrent', source: source);
}

class NexusAccepted {
  const NexusAccepted({
    required this.id,
    required this.collection,
    required this.queued,
  });

  final BigInt id;
  final int? collection;
  final bool queued;
}
