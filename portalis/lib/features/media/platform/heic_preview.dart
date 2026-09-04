import 'dart:collection';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Decodes HEIC/HEIF files with the operating system's image framework.
///
/// The returned JPEG is a bounded preview only. The source file remains at
/// its original path and is never rewritten or copied into app storage.
class HeicPreview {
  HeicPreview._();

  static const _channel = MethodChannel('app.portalis/heic-preview');
  static const _cacheLimit = 64;
  static final LinkedHashMap<String, Uint8List> _cache = LinkedHashMap();
  static final Map<String, Future<Uint8List?>> _inFlight = {};

  static bool get isSupported => switch (defaultTargetPlatform) {
        TargetPlatform.android ||
        TargetPlatform.iOS ||
        TargetPlatform.macOS =>
          true,
        _ => false,
      };

  static Future<Uint8List?> decode(String path, int maxPixelSize) async {
    if (!isSupported) return null;
    final size = maxPixelSize.clamp(64, 2048);
    final key = '$size\u0000$path';
    final cached = _cache.remove(key);
    if (cached != null) {
      _cache[key] = cached;
      return cached;
    }
    final running = _inFlight[key];
    if (running != null) return running;

    final loading = _decode(path, size);
    _inFlight[key] = loading;
    try {
      final bytes = await loading;
      if (bytes != null) {
        _cache[key] = bytes;
        while (_cache.length > _cacheLimit) {
          _cache.remove(_cache.keys.first);
        }
      }
      return bytes;
    } finally {
      if (identical(_inFlight[key], loading)) _inFlight.remove(key);
    }
  }

  static Future<Uint8List?> _decode(String path, int maxPixelSize) async {
    try {
      return await _channel.invokeMethod<Uint8List>('decode', {
        'path': path,
        'maxPixelSize': maxPixelSize,
      });
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }
}
