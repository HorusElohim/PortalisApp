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

import '../bridge/portalis_api.dart';

export '../bridge/portalis_api.dart'
    show
        AppAccepted,
        AppActivity,
        AppAppRun,
        AppCollection,
        AppCollectionCapabilities,
        AppCollectionFacts,
        AppCollectionLifecycle,
        AppCollectionNature,
        AppCollectionPeer,
        AppCollectionRole,
        AppContact,
        AppDetail,
        AppDevice,
        AppEntry,
        AppMember,
        AppPending,
        AppPeer,
        AppPeerHistory,
        AppPeoplePeer,
        AppPublishProgress,
        AppSnapshot,
        AppSourceFile,
        AppTransfer,
        AppTransferCompleted,
        AppUserSummary;


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

extension EntryPreview on AppEntry {
  /// Whether this entry can be shown as a picture.
  bool get isImage {
    final name = label.toLowerCase();
    return const ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.heic', '.bmp']
        .any(name.endsWith);
  }
}

/// Formatting derived from the engine's own aggregated activity — see
/// [AppActivity] in the generated bridge for why this is aggregated in Rust
/// rather than recomputed per screen.
extension EngineActivityPresentation on AppActivity {
  bool get isMoving => transfers > 0;

  /// Everything moving right now, in the unit the engine counts.
  int get totalBytesPerSecond => downBytesPerSecond + upBytesPerSecond;
}
