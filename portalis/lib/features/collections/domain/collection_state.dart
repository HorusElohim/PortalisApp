import '../../../nexus/domain/app_state.dart';

/// Compatibility names for presentation code while the generated Rust enums
/// are migrated through every widget. These are aliases, not second enums:
/// there is no parser and no independent lifecycle vocabulary in Dart.
typedef CollectionState = AppCollectionLifecycle;
typedef CollectionNature = AppCollectionNature;
typedef CollectionRole = AppCollectionRole;

/// Human-facing formatting for the generated Rust lifecycle.
///
/// Formatting belongs in Flutter; deciding which lifecycle applies belongs in
/// Rust. Unknown-string fallback disappeared with the string wire contract — a
/// newer enum variant makes codegen/compilation fail instead of silently
/// answering false to every capability question.
extension CollectionStatePresentation on AppCollectionLifecycle {
  String label(String raw) => switch (this) {
        AppCollectionLifecycle.resolvingMetadata => 'RESOLVING METADATA',
        AppCollectionLifecycle.waitingForSender => 'WAITING FOR SENDER',
        AppCollectionLifecycle.metadataReady => 'METADATA READY · CHOOSE FILES',
        AppCollectionLifecycle.downloadRequested => 'DOWNLOAD REQUESTED',
        AppCollectionLifecycle.retryingMetadata =>
          'METADATA TIMEOUT · RETRYING',
        AppCollectionLifecycle.waitingForOwner => 'WAITING FOR OWNER',
        AppCollectionLifecycle.accessRemoved => 'ACCESS REMOVED',
        AppCollectionLifecycle.needsNewerVersion => 'NEEDS NEWER VERSION',
        AppCollectionLifecycle.cannotVerify => 'CANNOT VERIFY',
        AppCollectionLifecycle.conflictingHistory => 'CONFLICTING HISTORY',
        _ => raw.toUpperCase(),
      };
}
