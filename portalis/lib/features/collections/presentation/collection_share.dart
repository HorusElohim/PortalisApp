import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
// image_picker also exports a (legacy, deprecated) PickedFile — this
// screen's own is the one everywhere else in the app means by that name.
import 'package:image_picker/image_picker.dart' hide PickedFile;

import '../../../app/app_controllers.dart';
import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../media/application/media_formats.dart';
import '../../../nexus/application/app_controller.dart';
import '../../../nexus/domain/app_state.dart';
import '../domain/picked_file.dart';
import '../platform/no_copy_source_picker.dart';
import '../platform/photo_library_picker.dart';
import '../platform/source_access.dart';

/// The registry already names every type it knows, so this reads the label
/// off the format rather than maintaining a second mapping that could drift.
String _kindLabel(String name) => MediaFormats.resolve(name).label;

/// "Share something" — the share half of the old combined Add screen,
/// redesigned per the Portalis Add Flow: inline-editable collection name,
/// Photos/Files/Folder pickers, a file list with per-item remove, and one
/// Create & share action that seeds the new collection from this device.
class ShareScreen extends StatefulWidget {
  const ShareScreen({
    super.key,
    this.onClose,
    this.initialFiles,
    this.controller,
  });

  /// Called instead of popping a route — set when this is embedded in the
  /// desktop shell's centre pane rather than pushed over it.
  final VoidCallback? onClose;

  /// Pre-populates the file list — set when this is reached by dropping
  /// files onto Home rather than by picking them here. The name still has to
  /// be typed either way, so a drop lands here rather than skipping this
  /// screen entirely.
  final List<PickedFile>? initialFiles;

  /// Injected by tests and alternate shells; production uses the one
  /// application-owned Nexus runtime.
  final NexusAppController? controller;

  @override
  State<ShareScreen> createState() => _ShareScreenState();
}

class _ShareScreenState extends State<ShareScreen> {
  final _nameController = TextEditingController();
  List<PickedFile> _files = [];
  bool _busy = false;
  String? _error;

  NexusAppController get _controller =>
      widget.controller ?? AppControllers.nexusApp;

  @override
  void initState() {
    super.initState();
    final initial = widget.initialFiles;
    if (initial != null && initial.isNotEmpty) {
      _files = initial;
    }
  }

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  void _add(Iterable<PickedFile> picked) {
    final existing = _files.map((f) => f.name).toSet();
    final additions = picked.where((f) => !existing.contains(f.name)).toList();
    setState(() {
      _error = null;
      _files = [..._files, ...additions];
    });
  }

  Future<void> _pickPhotos() async {
    if (supportsMobileGallerySources) {
      try {
        _add(await PhotoLibraryPicker.pickMedia());
      } catch (error) {
        _toast('Couldn\'t access those Photos items: $error',
            severity: ToastSeverity.error);
      }
      return;
    }
    if (!supportsDirectPathSources) {
      _toast(noCopySourceUnavailableMessage);
      return;
    }
    try {
      final xfiles = await ImagePicker().pickMultipleMedia();
      if (xfiles.isEmpty) return;
      _add(await Future.wait(xfiles.map((f) => pickedFileFrom(
            name: f.name,
            nativePath: f.path,
          ))));
    } catch (error) {
      _toast('Couldn\'t read those files: $error',
          severity: ToastSeverity.error);
    }
  }

  /// Files, or a folder's files, from one affordance.
  ///
  /// A person adding "these things" does not think of a folder as a different
  /// kind of act, and the result is identical either way: a flat list of
  /// no-copy sources. Two buttons made them choose a picker before choosing
  /// content; a sheet asks the question only when the platform can answer it
  /// both ways.
  Future<void> _pickFiles() async {
    if (supportsDirectPathSources) {
      final choice = await showModalBottomSheet<String>(
        context: context,
        backgroundColor: AppColors.surface,
        builder: (sheetContext) => SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              ListTile(
                key: const Key('shareChooseFiles'),
                leading: const Icon(Icons.description_outlined),
                title: const Text('Choose files'),
                onTap: () => Navigator.of(sheetContext).pop('files'),
              ),
              ListTile(
                key: const Key('shareChooseFolder'),
                leading: const Icon(Icons.folder_outlined),
                title: const Text('Choose a folder'),
                subtitle: const Text('Adds the files at its top level'),
                onTap: () => Navigator.of(sheetContext).pop('folder'),
              ),
            ],
          ),
        ),
      );
      if (choice == null || !mounted) return;
      if (choice == 'folder') {
        await _pickFolder();
        return;
      }
      await _pickIndividualFiles();
      return;
    }
    await _pickIndividualFiles();
  }

  Future<void> _pickIndividualFiles() async {
    if (supportsNativeFilesSources) {
      try {
        _add(await NoCopySourcePicker.pickFiles());
      } catch (error) {
        _toast('Couldn\'t access those files: $error',
            severity: ToastSeverity.error);
      }
      return;
    }
    if (!supportsDirectPathSources) {
      _toast(noCopySourceUnavailableMessage);
      return;
    }
    final result = await FilePicker.pickFiles(
      withData: false,
      allowMultiple: true,
      type: FileType.any,
    );
    if (result == null) return;
    try {
      _add(await Future.wait(result.files.map((f) => pickedFileFrom(
            name: f.name,
            nativePath: f.path,
          ))));
    } catch (error) {
      _toast('$error', severity: ToastSeverity.error);
    }
  }

  Future<void> _pickFolder() async {
    if (!supportsDirectPathSources) {
      _toast(noCopySourceUnavailableMessage);
      return;
    }
    try {
      final dir = await FilePicker.getDirectoryPath();
      if (dir == null) return;
      final entries = await Directory(dir)
          .list(recursive: false)
          .where((e) => e is File)
          .cast<File>()
          .toList();
      if (entries.isEmpty) {
        _toast('That folder has no files at its top level');
        return;
      }
      final picked = <PickedFile>[];
      for (final file in entries) {
        final basename = file.path.split(Platform.pathSeparator).last;
        picked.add(await pickedFileFrom(
          name: basename,
          nativePath: file.path,
        ));
      }
      _add(picked);
    } catch (e) {
      // Folder access is platform-dependent (sandboxing) — degrade to a
      // message rather than a crash; Photos/Files still work.
      _toast('Couldn\'t read that folder: $e');
    }
  }

  /// A `.torrent` descriptor or a magnet, which creates its own collection.
  ///
  /// Unlike every other source here it does not add to `_files`: its content
  /// is not on this device to be shared, it is something to fetch. So it
  /// bypasses the name field and the Create button entirely and hands the
  /// source to Nexus, which resolves it and asks which files to take.
  Future<void> _pickTorrent() async {
    final choice = await showModalBottomSheet<String>(
      context: context,
      backgroundColor: AppColors.surface,
      builder: (sheetContext) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              key: const Key('shareChooseTorrentFile'),
              leading: const Icon(Icons.description_outlined),
              title: const Text('Choose a .torrent file'),
              onTap: () => Navigator.of(sheetContext).pop('file'),
            ),
            ListTile(
              key: const Key('sharePasteMagnet'),
              leading: const Icon(Icons.link),
              title: const Text('Paste a magnet link'),
              onTap: () => Navigator.of(sheetContext).pop('magnet'),
            ),
          ],
        ),
      ),
    );
    if (choice == null || !mounted) return;

    final source = choice == 'magnet'
        ? await promptForText(
            context,
            title: 'Paste a magnet link',
            confirmLabel: 'Add',
          )
        : await _pickTorrentFilePath();
    if (source == null || source.trim().isEmpty || !mounted) return;
    await _importTorrent(source.trim());
  }

  Future<String?> _pickTorrentFilePath() async {
    try {
      final result = await FilePicker.pickFiles(
        withData: false,
        type: FileType.custom,
        allowedExtensions: ['torrent'],
      );
      return result?.files.single.path;
    } catch (error) {
      _toast('Couldn\'t read that .torrent file: $error',
          severity: ToastSeverity.error);
      return null;
    }
  }

  Future<void> _importTorrent(String source) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _controller.send(NexusCommand.importTorrent(source));
      if (!mounted) return;
      _close();
      showToast(
        context,
        'Resolving the torrent — choose files next',
        severity: ToastSeverity.success,
      );
    } catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Always offered, on every platform: importing a torrent is a download,
  /// not a share, so it does not depend on this device having a no-copy
  /// picker for its own files.
  Widget _torrentButton() => _PickerButton(
        key: const Key('shareAddTorrent'),
        label: 'Torrent',
        icon: Icons.download_outlined,
        onTap: _busy ? null : _pickTorrent,
      );

  void _toast(String msg, {ToastSeverity severity = ToastSeverity.info}) {
    if (!mounted) return;
    showToast(context, msg, severity: severity);
  }

  /// Pops the pushed route, or — embedded in the desktop shell — hands
  /// control back to whatever put this on screen.
  void _close() =>
      widget.onClose != null ? widget.onClose!() : Navigator.of(context).pop();

  Future<void> _createShare() async {
    if (!supportsNoCopySources) {
      setState(() => _error = noCopySourceUnavailableMessage);
      return;
    }
    final name = _nameController.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Name the collection before creating it');
      return;
    }
    if (_files.isEmpty) {
      setState(() => _error = 'Add at least one file');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _controller.send(
        NexusCommand(
          kind: 'createCollection',
          name: name,
          files: _files
              .map(
                (file) => AppSourceFile(
                  name: file.name,
                  path: file.path,
                  bytes: BigInt.from(file.lengthBytes),
                ),
              )
              .toList(growable: false),
        ),
      );
      if (mounted) {
        FocusScope.of(context).unfocus();
        _close();
        _toastGlobal('"$name" is preparing in the background');
      }
    } catch (e) {
      setState(() => _error = 'Couldn\'t create collection: $e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Shown after this screen pops. showToast targets the *root* overlay, so
  /// unlike the old ScaffoldMessenger call it survives the navigation that
  /// triggered it.
  void _toastGlobal(String msg) {
    showToast(context, msg, severity: ToastSeverity.success);
  }

  int get _totalBytes => _files.fold(0, (sum, f) => sum + f.lengthBytes);

  @override
  Widget build(BuildContext context) {
    final name = _nameController.text.trim();
    final canCreate = !_busy && name.isNotEmpty && _files.isNotEmpty;
    final summary = _files.isEmpty
        ? 'Add at least one item'
        : name.isEmpty
            ? 'Name the collection to continue'
            : 'Seeds from this device as soon as it\'s created';

    return AppScreen(
      title: 'New share',
      subtitle: Text(
        supportsDirectPathSources
            ? 'Files stay on this device — collaborators pull them from you.'
            : supportsMobileGallerySources
                ? 'Choose original photos and videos from your library. '
                    'Portalis reads them in place and does not import a copy.'
                : supportsNativeFilesSources
                ? 'Choose files in Files. Portalis keeps the original location '
                    'and never imports a second copy.'
                : noCopySourceUnavailableMessage,
      ),
      onBack: _close,
      body: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(kScreenGutter, 0, kScreenGutter, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // A labelled field rather than a 25pt look-alike title.
            // It used to sit where the screen title goes, which is
            // why it needed a "tap the name to rename" hint to
            // explain that it was editable at all.
            const SectionLabel('COLLECTION NAME'),
            const SizedBox(height: 7),
            TextField(
              key: const Key('collectionNameField'),
              controller: _nameController,
              style: AppText.body(),
              cursorColor: AppColors.signal,
              decoration: InputDecoration(
                hintText: 'Untitled collection',
                hintStyle: TextStyle(color: AppColors.textGhost),
                filled: true,
                fillColor: AppColors.surface,
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 16),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppRadius.control),
                  borderSide: BorderSide(color: AppColors.borderStrong),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppRadius.control),
                  borderSide: BorderSide(color: AppColors.borderStrong),
                ),
              ),
              onChanged: (_) => setState(() {}),
            ),
            const SizedBox(height: 18),
            // Local sources are platform-gated: sharing files this device
            // holds needs a no-copy picker, and not every platform has one.
            if (supportsDirectPathSources)
              Row(
                children: [
                  Expanded(
                    child: _PickerButton(
                      label: 'Photos',
                      icon: Icons.photo_camera_outlined,
                      onTap: _busy ? null : _pickPhotos,
                    ),
                  ),
                  const SizedBox(width: 9),
                  Expanded(
                    child: _PickerButton(
                      label: 'Files',
                      icon: Icons.description_outlined,
                      onTap: _busy ? null : _pickFiles,
                    ),
                  ),
                  const SizedBox(width: 9),
                  Expanded(child: _torrentButton()),
                ],
              )
            else if (supportsNativeFilesSources || supportsMobileGallerySources)
              Row(
                children: [
                  if (supportsMobileGallerySources)
                    Expanded(
                      child: _PickerButton(
                        label: 'Photos & videos',
                        icon: Icons.photo_library_outlined,
                        onTap: _busy ? null : _pickPhotos,
                      ),
                    ),
                  if (supportsMobileGallerySources && supportsNativeFilesSources)
                    const SizedBox(width: 9),
                  if (supportsNativeFilesSources)
                    Expanded(
                      child: _PickerButton(
                        label: 'Files',
                        icon: Icons.folder_open_outlined,
                        onTap: _busy ? null : _pickFiles,
                      ),
                    ),
                  const SizedBox(width: 9),
                  Expanded(child: _torrentButton()),
                ],
              )
            else ...[
              // Nothing local can be shared here, but a torrent is fetched
              // rather than shared — it needs no picker at all, so it stays
              // available where the others cannot be.
              _torrentButton(),
              const SizedBox(height: 14),
              _NoCopySourceNotice(message: noCopySourceUnavailableMessage),
            ],
            const SizedBox(height: 14),
            if (_files.isEmpty)
              Container(
                height: 150,
                decoration: BoxDecoration(
                  border: Border.all(color: AppColors.borderStrong),
                  borderRadius: BorderRadius.circular(AppRadius.control),
                  color: AppColors.surface.withValues(alpha: 0.4),
                ),
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Icon(Icons.upload_outlined,
                        size: 26, color: AppColors.textGhost),
                    SizedBox(height: 8),
                    Text(
                      'Nothing added yet',
                      style: AppText.body(color: AppColors.textDim),
                    ),
                  ],
                ),
              )
            else ...[
              Row(
                children: [
                  Text(
                    '${_files.length} ITEM${_files.length == 1 ? '' : 'S'} · ${formatBytes(_totalBytes)}',
                    style: monoLabel(
                        size: 10.5,
                        color: AppColors.textDim,
                        letterSpacing: 1.0),
                  ),
                  const Spacer(),
                  TextButton(
                    onPressed: () => setState(() => _files = []),
                    child: Text(
                      'Remove all',
                      style: AppText.secondary(color: AppColors.signalSoft),
                    ),
                  ),
                ],
              ),
              for (final f in _files) _fileRow(f),
            ],
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 14),
                child: Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
                  decoration: BoxDecoration(
                    color: const Color(0xFFEB5757).withValues(alpha: 0.1),
                    border: Border.all(
                        color: const Color(0xFFEB5757).withValues(alpha: 0.4)),
                    borderRadius: BorderRadius.circular(AppRadius.tight),
                  ),
                  child: Text(
                    _error!,
                    style: AppText.caption(color: Color(0xFFEB5757)),
                  ),
                ),
              ),
          ],
        ),
      ),
      footer: ScreenAction(
        buttonKey: const Key('createShareButton'),
        label: 'Create & share',
        onPressed: canCreate ? _createShare : null,
        busy: _busy,
        hint: summary,
      ),
    );
  }

  Widget _fileRow(PickedFile f) {
    final dot = f.name.lastIndexOf('.');
    final ext = dot == -1 || dot == f.name.length - 1
        ? 'FILE'
        : f.name.substring(dot + 1).toUpperCase();
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 8),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: AppColors.border)),
      ),
      child: Row(
        children: [
          Container(
            width: 34,
            height: 34,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: AppColors.surface,
              border: Border.all(color: AppColors.border),
              borderRadius: BorderRadius.circular(AppRadius.tight),
            ),
            child: Text(
              ext.length > 4 ? ext.substring(0, 4) : ext,
              style: monoLabel(
                size: 9,
                color: AppColors.textDim,
                letterSpacing: 0.6,
                weight: FontWeight.w600,
              ),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  f.name,
                  overflow: TextOverflow.ellipsis,
                  style: AppText.body(),
                ),
                const SizedBox(height: 1),
                Text(
                  '${_kindLabel(f.name)} · ${formatBytes(f.lengthBytes)}',
                  style: AppText.caption(color: AppColors.textDim),
                ),
              ],
            ),
          ),
          IconButton(
            onPressed: () => setState(
                () => _files = _files.where((other) => other != f).toList()),
            icon: Icon(Icons.close, size: 15, color: AppColors.textDim),
          ),
        ],
      ),
    );
  }
}

class _PickerButton extends StatelessWidget {
  const _PickerButton({
    super.key,
    required this.label,
    required this.icon,
    this.onTap,
  });

  final String label;
  final IconData icon;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: AppColors.surface,
      borderRadius: BorderRadius.circular(AppRadius.inner),
      child: InkWell(
        borderRadius: BorderRadius.circular(AppRadius.inner),
        onTap: onTap,
        child: Container(
          height: 74,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(AppRadius.inner),
            border: Border.all(color: AppColors.borderStrong),
          ),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, size: 20, color: AppColors.text),
              const SizedBox(height: 7),
              Text(label, style: AppText.secondary(color: AppColors.text)),
            ],
          ),
        ),
      ),
    );
  }
}

class _NoCopySourceNotice extends StatelessWidget {
  const _NoCopySourceNotice({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: AppColors.surface,
          border: Border.all(color: AppColors.borderStrong),
          borderRadius: BorderRadius.circular(AppRadius.inner),
        ),
        child: Text(message, style: AppText.caption(color: AppColors.textDim)),
      );
}
