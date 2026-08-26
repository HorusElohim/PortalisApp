import '../../../nexus/domain/app_state.dart';

/// What a collection is doing, as the backend named it.
///
/// The backend sends one word per collection and the interface used to compare
/// it against string literals wherever a decision was needed. That put the
/// spelling of a Rust enum in a dozen widgets, and it rotted exactly as you
/// would expect: three sites tested for `'downloading'` and `'importing'`,
/// which the backend has never sent, so they were quietly always false.
///
/// Parsing once, here, makes an unrecognised word visible as [unknown] instead
/// of silently answering `false` to every question. See `projection/wire.rs`
/// for the other half of the contract.
enum CollectionState {
  available,
  seeding,
  paused,
  draft,
  preparing,
  downloading,
  updating,
  waitingForOwner,
  accessRemoved,
  needsNewerVersion,
  cannotVerify,
  conflictingHistory,

  /// A word this build does not know. Reported rather than guessed: a newer
  /// backend saying something new is a real possibility, and treating it as
  /// any particular state would be the interface inventing a fact.
  unknown;

  /// The word the backend uses for this state, or `null` for [unknown], which
  /// names no single one.
  String? get wire => switch (this) {
        CollectionState.available => 'Available',
        CollectionState.seeding => 'Seeding',
        CollectionState.paused => 'Paused',
        CollectionState.draft => 'Draft',
        CollectionState.preparing => 'Preparing',
        CollectionState.downloading => 'Downloading',
        CollectionState.updating => 'Updating',
        CollectionState.waitingForOwner => 'WaitingForOwner',
        CollectionState.accessRemoved => 'AccessRemoved',
        CollectionState.needsNewerVersion => 'NeedsNewerVersion',
        CollectionState.cannotVerify => 'CannotVerify',
        CollectionState.conflictingHistory => 'ConflictingHistory',
        CollectionState.unknown => null,
      };

  static CollectionState parse(String word) => switch (word) {
        'Available' => CollectionState.available,
        'Seeding' => CollectionState.seeding,
        'Paused' => CollectionState.paused,
        'Draft' => CollectionState.draft,
        'Preparing' => CollectionState.preparing,
        'Downloading' => CollectionState.downloading,
        'Updating' => CollectionState.updating,
        'WaitingForOwner' => CollectionState.waitingForOwner,
        'AccessRemoved' => CollectionState.accessRemoved,
        'NeedsNewerVersion' => CollectionState.needsNewerVersion,
        'CannotVerify' => CollectionState.cannotVerify,
        'ConflictingHistory' => CollectionState.conflictingHistory,
        _ => CollectionState.unknown,
      };

  /// What to show a person who is looking at this collection.
  ///
  /// [unknown] falls back to the backend's own word rather than a placeholder:
  /// if a newer backend says something this build cannot interpret, showing it
  /// verbatim is more use than showing nothing.
  String label(String raw) => switch (this) {
        CollectionState.waitingForOwner => 'WAITING FOR OWNER',
        CollectionState.accessRemoved => 'ACCESS REMOVED',
        CollectionState.needsNewerVersion => 'NEEDS NEWER VERSION',
        CollectionState.cannotVerify => 'CANNOT VERIFY',
        CollectionState.conflictingHistory => 'CONFLICTING HISTORY',
        _ => raw.toUpperCase(),
      };
}

/// How a collection's content entered Portalis.
enum CollectionNature {
  native,
  torrent,
  unknown;

  static CollectionNature parse(String word) => switch (word) {
        'Native' => CollectionNature.native,
        'Torrent' => CollectionNature.torrent,
        _ => CollectionNature.unknown,
      };
}

/// Whether this device publishes a collection or reads it.
enum CollectionRole {
  owner,
  member,
  unknown;

  static CollectionRole parse(String word) => switch (word) {
        'Owner' => CollectionRole.owner,
        'Member' => CollectionRole.member,
        _ => CollectionRole.unknown,
      };
}

extension AppCollectionContract on AppCollection {
  CollectionState get lifecycle => CollectionState.parse(status);
  CollectionNature get kind => CollectionNature.parse(nature);
  CollectionRole get ownership => CollectionRole.parse(role);
}
