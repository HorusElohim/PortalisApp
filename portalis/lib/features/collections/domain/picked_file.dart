import 'dart:io';

import '../platform/security_scoped_sources.dart';

/// A native file selected by the UI. Only identity and display metadata live
/// in Dart; Rust owns opening, copying, hashing, and publishing its contents.
class PickedFile {
  const PickedFile({
    required this.name,
    required this.path,
    required this.lengthBytes,
  });

  final String name;
  final String path;
  final int lengthBytes;
}

Future<PickedFile> pickedFileFrom({
  required String name,
  required String? nativePath,
}) async {
  if (nativePath == null || nativePath.isEmpty) {
    throw UnsupportedError('The selected file has no native path');
  }
  final length = await File(nativePath).length();
  // Claimed here rather than at publication: this is the moment the app is
  // still allowed to, and Portalis seeds the original file for as long as the
  // collection exists — including after a restart, which is precisely the
  // access a sandbox otherwise withdraws.
  await SecurityScopedSources.retain([nativePath]);
  return PickedFile(
    name: name,
    path: nativePath,
    lengthBytes: length,
  );
}
