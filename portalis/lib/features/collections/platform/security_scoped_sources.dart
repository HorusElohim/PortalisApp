import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Keeps a sandboxed desktop app's permission to read the files it seeds.
///
/// A macOS sandbox grants access to a chosen file only for the life of the
/// process that chose it. Portalis never copies media — Rust reads the
/// person's original file every time it hashes or serves a piece — so without
/// a security-scoped bookmark, reopening the app lost access to everything it
/// was sharing and the backend failed with "Operation not permitted".
///
/// Only macOS needs this. iOS keeps its own bookmarks inside its Files picker,
/// Android holds persisted SAF permissions, and Linux and Windows are not
/// sandboxed this way — so everywhere else these calls are a no-op rather than
/// a second mechanism to keep in step.
class SecurityScopedSources {
  SecurityScopedSources._();

  static const _channel = MethodChannel('app.portalis/security-scoped-sources');

  static bool get _supported =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.macOS;

  /// Records durable access to freshly chosen files.
  ///
  /// Called while the app still holds the access the selection granted: the
  /// platform cannot mint a bookmark for a file it can no longer reach, so
  /// deferring this until publication would be too late.
  ///
  /// A file the platform refuses to retain is dropped from the result rather
  /// than failing the whole selection — the rest are still shareable, and the
  /// caller decides what to say about the remainder.
  static Future<List<String>> retain(List<String> paths) async {
    if (!_supported || paths.isEmpty) return paths;
    try {
      final retained = await _channel.invokeListMethod<String>(
        'retain',
        {'paths': paths},
      );
      return retained ?? const [];
    } on PlatformException catch (error) {
      debugPrint('[sources] could not retain access: ${error.message}');
      return const [];
    } on MissingPluginException {
      // An older host binary without the channel. Selection still works for
      // this run; only persistence across a restart is unavailable.
      return paths;
    }
  }

  /// Gives up access to files Portalis no longer seeds, so a deleted
  /// collection does not keep a permission alive for the rest of the install.
  static Future<void> release(List<String> paths) async {
    if (!_supported || paths.isEmpty) return;
    try {
      await _channel.invokeMethod<void>('release', {'paths': paths});
    } on PlatformException catch (error) {
      debugPrint('[sources] could not release access: ${error.message}');
    } on MissingPluginException {
      // Nothing was retained by a host without the channel.
    }
  }
}
