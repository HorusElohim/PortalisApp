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

import 'package:flutter/material.dart';
import '../../design/formatters.dart';
import '../../design/theme.dart';

import '../bridge/portalis_api.dart';

export '../bridge/portalis_api.dart'
    show
        AppAccepted,
        AppCommand,
        AppCollection,
        AppContact,
        AppDetail,
        AppDevice,
        AppEntry,
        AppPending,
        AppSnapshot,
        AppSourceFile,
        AppTransfer;

/// The piece map a detail carries, decoded.
///
/// An extension rather than a class, because the bytes come from the engine
/// and only their reading is the app's business. Codegen owns the fields; this
/// owns what they mean.
extension DetailPieces on AppDetail {
  /// Whether the bar at [index] is verified.
  bool pieceAt(int index) {
    final byte = index ~/ 8;
    if (byte < 0 || byte >= pieces.length) {
      return false;
    }
    return pieces[byte] & (1 << (index % 8)) != 0;
  }

  /// How many bars the piece map carries.
  int get pieceCount => pieces.length * 8;
}

/// One recorded reading of a transfer.
///
/// The history is its own stream now, arriving as the rows a subscriber has
/// not seen yet rather than as the whole ring — see `watch_history`. Decoded
/// once, on arrival, instead of on every read: `progress` used to walk
/// eighteen hundred rows to reach the last one, and it is read once per frame.
class Reading {
  const Reading({
    required this.at,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.progress,
  });

  final DateTime at;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final double progress;
}

/// Decodes packed readings, oldest first.
///
/// Rows are `at_unix_ns(8) | down(4) | up(4) | progress_permille(2)`,
/// big-endian, packed by the core. Recorded there rather than accumulated
/// from what the app happened to observe: the core records whether or not a
/// screen is open, so this history survives a restart and every screen sees
/// the same one.
List<Reading> decodeReadings(Uint8List packed) {
  const rowBytes = 18;
  final rows = <Reading>[];
  for (var offset = 0; offset + rowBytes <= packed.length; offset += rowBytes) {
    final row = ByteData.sublistView(packed, offset, offset + rowBytes);
    rows.add(Reading(
      at: DateTime.fromMicrosecondsSinceEpoch(row.getUint64(0) ~/ 1000),
      downBytesPerSecond: row.getUint32(8),
      upBytesPerSecond: row.getUint32(12),
      progress: row.getUint16(16) / 1000,
    ));
  }
  return rows;
}

extension AppEntryPresentation on AppEntry {
  int get sizeBytes => bytes.toInt();
  int get downloadedBytesInt => downloadedBytes.toInt();
  double get progress =>
      sizeBytes == 0 ? 0 : (downloadedBytesInt / sizeBytes).clamp(0.0, 1.0);
  bool get fetched => available || downloadedBytesInt > 0;
  bool get isReady => available && path != null;
  String? get localPath => isReady ? path : null;
}

extension AppCollectionPresentation on AppCollection {
  String get stringId => '$id';
  bool get isTorrent => nature == 'Torrent';
  bool get isShared => !isTorrent;
  bool get isDraft => status == 'Draft';
  bool get isPaused => status == 'Paused';
  bool get isConnecting => status == 'WaitingForOwner';
  bool get isDownloading => status == 'Downloading';
  bool get isComplete => status == 'Available';
  int get totalBytesInt => totalBytes.toInt();
  int get downloadedBytesInt => onDiskBytes.toInt();
  int get uploadedBytesInt => uploadedBytes.toInt();
  int get revisionInt => revision.toInt();
  double progressFor(Reading? lastReading) =>
      transfer?.progress ?? lastReading?.progress ?? 0;
  int get downBytesPerSecond => transfer?.downBytesPerSecond ?? 0;
  int get upBytesPerSecond => transfer?.upBytesPerSecond ?? 0;
  int livePeersFor(AppDetail? detail) =>
      transfer?.peers ?? detail?.peers.length ?? 0;
  bool isSharingFor(AppDetail? detail) => isComplete && entries > 0;
  int pendingEntries(AppDetail? detail) =>
      detail?.entries.where((entry) => !entry.available).length ?? 0;
  String? get etaLabel => transfer?.etaSecs == null
      ? null
      : '${formatEta(transfer!.etaSecs!)} left';
  Color get hue => AppColors.hueAt(stringId.hashCode.abs());
  GlowLevel glowFor(AppDetail? detail) {
    if (downBytesPerSecond > 0 || upBytesPerSecond > 0) {
      return downBytesPerSecond + upBytesPerSecond > 500000
          ? GlowLevel.vivid
          : GlowLevel.active;
    }
    return isConnecting || isSharingFor(detail)
        ? GlowLevel.calm
        : GlowLevel.none;
  }

  double get liveIntensity =>
      Glow.intensityForRate(downBytesPerSecond + upBytesPerSecond);

  DateTime? get startedAtMoment => _moment(startedAt);
  DateTime? get completedAtMoment => _moment(completedAt);

  String subtitleFor(AppDetail? detail) {
    final count = detail?.entries.length ?? entries;
    final items = '$count item${count == 1 ? '' : 's'}';
    if (isConnecting) return '$items · looking for a peer';
    final eta = etaLabel;
    if (eta != null) return '$items · $eta';
    final pending = pendingEntries(detail);
    return pending > 0 ? '$items · $pending to fetch' : items;
  }

  String copiesLabelFor(AppDetail? detail) {
    final peers = livePeersFor(detail);
    final peersLabel = '$peers peer${peers == 1 ? '' : 's'}';
    if (!isComplete) {
      final eta = etaLabel;
      final completed = formatProgressPercent(progressFor(null));
      return eta == null
          ? '$completed · $peersLabel'
          : '$completed · $eta · $peersLabel';
    }
    return peers == 0
        ? 'Seeding · this device'
        : 'Seeding · this device + $peersLabel';
  }

  List<AppContact> collaboratorsIn(List<AppContact> contacts) => [
        for (final member in members)
          for (final contact in contacts.where((item) => item.id == member))
            contact,
      ];
}

DateTime? _moment(BigInt? unixNanoseconds) => unixNanoseconds == null
    ? null
    : DateTime.fromMicrosecondsSinceEpoch(
        (unixNanoseconds ~/ BigInt.from(1000)).toInt(),
      );

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
class EngineActivity {
  const EngineActivity({
    required this.transfers,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.peers,
  });

  static const idle = EngineActivity(
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

  /// Everything moving right now, in the unit the engine counts.
  int get totalBytesPerSecond => downBytesPerSecond + upBytesPerSecond;
}
