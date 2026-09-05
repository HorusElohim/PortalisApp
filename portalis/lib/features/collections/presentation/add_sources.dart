import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
// image_picker also exports a (legacy, deprecated) PickedFile — this file's
// own PickedFile is the domain type, so the package's is hidden.
import 'package:image_picker/image_picker.dart' hide PickedFile;

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../domain/picked_file.dart';
import '../platform/no_copy_source_picker.dart';
import '../platform/photo_library_picker.dart';
import '../platform/source_access.dart';
import '../../peer_hints/widgets/collection_qr_scanner.dart';

/// What a person chose to make a collection out of.
///
/// Two shapes because there are two: files this device already holds and can
/// offer, and a torrent whose content is somewhere else and has to be
/// fetched. Everything downstream treats them differently, so the difference
/// is in the type rather than in a flag somebody could forget to read.
sealed class ChosenSources {
  const ChosenSources();
}

/// Files on this device, to be shared without copying them.
class LocalSources extends ChosenSources {
  const LocalSources(this.files);

  final List<PickedFile> files;
}

/// A magnet URI or a `.torrent` path, to be resolved and fetched.
class TorrentSource extends ChosenSources {
  const TorrentSource(this.source);

  final String source;
}

/// A collection QR link, imported and fetched rather than shared.
class ScannedCollectionSource extends ChosenSources {
  const ScannedCollectionSource(this.source);

  final String source;
}

/// Asks what to put in a new collection, and returns it.
///
/// This replaced a whole New Share page. The page existed to hold a name
/// field, a picker row and a file list — but the collection screen shows all
/// three better, and now that a collection can be a draft it can show them for
/// something that is not shared yet. What was left was a question with three
/// answers, which is a sheet.
///
/// Returns `null` when the person backed out at any point, including out of
/// the platform picker itself: choosing nothing is not the same as choosing an
/// empty collection.
Future<ChosenSources?> showAddSourcesSheet(BuildContext context) async {
  final choice = await showModalBottomSheet<String>(
    context: context,
    backgroundColor: AppColors.surface,
    shape: const RoundedRectangleBorder(
      borderRadius: BorderRadius.vertical(top: Radius.circular(AppRadius.card)),
    ),
    builder: (sheetContext) => SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const SizedBox(height: 6),
          Container(
            width: 36,
            height: 4,
            decoration: BoxDecoration(
              color: AppColors.borderStrong,
              borderRadius: BorderRadius.circular(AppRadius.pill),
            ),
          ),
          const SizedBox(height: 8),
          if (supportsMobileGallerySources || supportsDirectPathSources)
            ListTile(
              key: const Key('addPhotos'),
              leading: const Icon(Icons.photo_library_outlined),
              title: const Text('Photos & videos'),
              onTap: () => Navigator.of(sheetContext).pop('photos'),
            ),
          ListTile(
            key: const Key('addFiles'),
            leading: const Icon(Icons.description_outlined),
            title: const Text('Files'),
            subtitle: const Text('A .torrent here is fetched, not shared'),
            onTap: () => Navigator.of(sheetContext).pop('files'),
          ),
          if (supportsDirectPathSources)
            ListTile(
              key: const Key('addFolder'),
              leading: const Icon(Icons.folder_outlined),
              title: const Text('A folder'),
              subtitle: const Text('Adds the files at its top level'),
              onTap: () => Navigator.of(sheetContext).pop('folder'),
            ),
          ListTile(
            key: const Key('addMagnet'),
            leading: const Icon(Icons.link),
            title: const Text('Magnet link'),
            onTap: () => Navigator.of(sheetContext).pop('magnet'),
          ),
          ListTile(
            key: const Key('scanCollectionQr'),
            leading: const Icon(Icons.qr_code_scanner_outlined),
            title: const Text('Scan QR code'),
            subtitle: const Text('Import a shared collection'),
            onTap: () => Navigator.of(sheetContext).pop('scanQr'),
          ),
          const SizedBox(height: 6),
        ],
      ),
    ),
  );
  if (choice == null || !context.mounted) return null;
  if (choice == 'magnet') {
    // Typed rather than opened, so it needs a field — asked for here, while
    // this surface is certainly still current, rather than inside a picker
    // that has no business holding one.
    final source = await promptForText(
      context,
      title: 'Paste a magnet link',
      confirmLabel: 'Add',
    );
    final trimmed = source?.trim() ?? '';
    return trimmed.isEmpty ? null : TorrentSource(trimmed);
  }

  if (choice == 'scanQr') {
    final scanned = await scanCollectionQrCode(context);
    if (scanned == null || !context.mounted) return null;
    return ScannedCollectionSource(scanned);
  }

  try {
    return switch (choice) {
      'photos' => await _pickPhotos(),
      'files' => await _pickFiles(),
      'folder' => await _pickFolder(),
      _ => null,
    };
  } on PickerFailure catch (failure) {
    // The one place a picker's failure meets a screen. The pickers take no
    // context of their own, so none of them can outlive the surface that
    // asked — which is the whole reason this is the only catch.
    if (context.mounted) {
      showToast(context, failure.message, severity: ToastSeverity.error);
    }
    return null;
  }
}

/// A picker could not do what was asked, in words worth showing.
class PickerFailure implements Exception {
  const PickerFailure(this.message);

  final String message;

  @override
  String toString() => message;
}

Future<ChosenSources?> _pickPhotos() async {
  if (supportsMobileGallerySources) {
    try {
      return LocalSources(await PhotoLibraryPicker.pickMedia());
    } catch (error) {
      throw PickerFailure("Couldn't access those Photos items: $error");
    }
  }
  if (!supportsDirectPathSources) {
    throw PickerFailure(noCopySourceUnavailableMessage);
  }
  try {
    final picked = await ImagePicker().pickMultipleMedia();
    if (picked.isEmpty) return null;
    return LocalSources(await Future.wait(picked.map(
      (file) => pickedFileFrom(name: file.name, nativePath: file.path),
    )));
  } catch (error) {
    throw PickerFailure("Couldn't read those files: $error");
  }
}

/// Files, or one `.torrent` among them.
///
/// A person picking a `.torrent` has not chosen a file to share — they have
/// named something to fetch. Deciding that here, from the extension, means
/// they never have to know which of two buttons the file belongs behind.
Future<ChosenSources?> _pickFiles() async {
  if (supportsNativeFilesSources) {
    try {
      final picked = await NoCopySourcePicker.pickFiles();
      return _asTorrentIfSingleDescriptor(picked) ?? LocalSources(picked);
    } catch (error) {
      throw PickerFailure("Couldn't access those files: $error");
    }
  }
  if (!supportsDirectPathSources) {
    throw PickerFailure(noCopySourceUnavailableMessage);
  }
  final files = await FilePicker.pickFiles(
    type: FileType.any,
  );
  if (files.isEmpty) return null;
  try {
    final picked = await Future.wait(files.map(
      (file) => pickedFileFrom(name: file.name, nativePath: file.path),
    ));
    return _asTorrentIfSingleDescriptor(picked) ?? LocalSources(picked);
  } catch (error) {
    throw PickerFailure('$error');
  }
}

/// A lone `.torrent` is an import; a `.torrent` among other files is not.
///
/// Picking twenty files one of which happens to be a descriptor is somebody
/// sharing twenty files, and silently fetching one of them instead would be
/// the surprising reading of an unambiguous act.
ChosenSources? _asTorrentIfSingleDescriptor(List<PickedFile> picked) {
  if (picked.length != 1) return null;
  final only = picked.single;
  if (!only.name.toLowerCase().endsWith('.torrent')) return null;
  return TorrentSource(only.path);
}

Future<ChosenSources?> _pickFolder() async {
  if (!supportsDirectPathSources) {
    throw PickerFailure(noCopySourceUnavailableMessage);
  }
  try {
    final directory = await FilePicker.getDirectoryPath();
    if (directory == null) return null;
    final entries = await Directory(directory)
        .list(recursive: false)
        .where((entry) => entry is File)
        .cast<File>()
        .toList();
    if (entries.isEmpty) {
      throw const PickerFailure('That folder has no files at its top level');
    }
    return LocalSources([
      for (final file in entries)
        await pickedFileFrom(
          name: file.path.split(Platform.pathSeparator).last,
          nativePath: file.path,
        ),
    ]);
  } on PickerFailure {
    rethrow;
  } catch (error) {
    // Folder access is platform-dependent (sandboxing) — degrade to a message
    // rather than a crash; Photos and Files still work.
    throw PickerFailure("Couldn't read that folder: $error");
  }
}
