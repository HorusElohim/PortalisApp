import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path_provider_platform_interface/path_provider_platform_interface.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';
import 'package:portalis/features/settings/presentation/diagnostics_screen.dart';

/// Reports a temp directory path that does not exist on disk yet — the
/// real-world condition that broke Share: on some platforms (observed on
/// macOS) the sandboxed Caches directory `getTemporaryDirectory()` names
/// is not guaranteed to already exist, and writing straight into it failed
/// with `PathNotFoundException`.
class _MissingTempDir extends PathProviderPlatform
    with MockPlatformInterfaceMixin {
  _MissingTempDir(this.path);

  final String path;

  @override
  Future<String?> getTemporaryPath() async => path;
}

void main() {
  group('writeShareFile', () {
    test('creates its temp directory when the platform reports one that '
        'does not exist yet', () async {
      final scratch =
          Directory.systemTemp.createTempSync('portalis-share-test-');
      addTearDown(() => scratch.deleteSync(recursive: true));
      final missing = Directory('${scratch.path}/does-not-exist-yet');
      expect(missing.existsSync(), isFalse,
          reason: 'the test only proves anything if this starts missing');
      PathProviderPlatform.instance = _MissingTempDir(missing.path);

      final file = await writeShareFile('a diagnostics line\n');

      expect(missing.existsSync(), isTrue,
          reason: 'the directory is created before writing into it');
      expect(file.existsSync(), isTrue);
      expect(await file.readAsString(), 'a diagnostics line\n');
    });

    test('overwrites rather than appends on a second share', () async {
      final scratch =
          Directory.systemTemp.createTempSync('portalis-share-test-');
      addTearDown(() => scratch.deleteSync(recursive: true));
      PathProviderPlatform.instance = _MissingTempDir(scratch.path);

      await writeShareFile('first');
      final file = await writeShareFile('second');

      expect(await file.readAsString(), 'second');
    });
  });
}
