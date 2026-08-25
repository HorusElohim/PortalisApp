import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../domain/picked_file.dart';

/// Opens an iOS Files document or Android SAF document in place.
///
/// The native picker returns only stable, access-scoped source metadata.
/// It never materialises file bytes in Dart or a plugin cache. Rust can then
/// hash and seed that same location directly.
class NoCopySourcePicker {
  NoCopySourcePicker._();

  static const _channel = MethodChannel('app.portalis/no-copy-source-picker');

  static Future<List<PickedFile>> pickFiles() async {
    if (defaultTargetPlatform != TargetPlatform.iOS &&
        defaultTargetPlatform != TargetPlatform.android) {
      throw UnsupportedError(
          'The native Files picker is only available on iOS and Android');
    }
    final values = await _channel.invokeListMethod<dynamic>('pickFiles') ?? [];
    return values.map(_fromNative).toList(growable: false);
  }

  static PickedFile _fromNative(dynamic value) {
    if (value is! Map) {
      throw const FormatException(
          'Native Files picker returned an invalid item');
    }
    final item = Map<Object?, Object?>.from(value);
    final name = item['name'];
    final path = item['path'];
    final length = item['lengthBytes'];
    if (name is! String || path is! String || length is! int) {
      throw const FormatException(
          'Native Files picker returned invalid metadata');
    }
    return PickedFile(name: name, path: path, lengthBytes: length);
  }
}
