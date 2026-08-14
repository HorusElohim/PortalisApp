import 'dart:async';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:image_picker/image_picker.dart' hide PickedFile;
import 'package:qr_flutter/qr_flutter.dart';

import '../../../design/collection_deletion_dialog.dart';
import '../../../design/design.dart';
import '../../../design/resizable_media_preview.dart';
import '../../media/domain/media_item.dart';
import '../../media/presentation/media_viewer_screen.dart';
import '../domain/collection.dart';
import '../domain/picked_file.dart';
import '../platform/no_copy_source_picker.dart';
import '../platform/photo_library_picker.dart';
import '../platform/source_access.dart';
import 'collection_contents.dart';
import 'collection_commands.dart';
import 'collection_overview.dart';
import 'collection_source.dart';
import '../../../theme.dart';

/// Shows one collection and coordinates user actions with whichever
/// [CollectionSource] backs it. Collection-specific rendering lives in the
/// presentation layer; where a reading comes from and where a command lands
/// is the source's business, not this widget's.
class CollectionDetail extends StatefulWidget {
  const CollectionDetail({
    super.key,
    required this.collection,
    required this.source,
    this.showCommands = true,
    this.level = CollectionDetailLevel.full,
    this.showTitle = true,
    this.inlineHeader,
    this.inlineStatus,
  });

  final Collection collection;

  /// Where this collection's live state comes from, and where its commands
  /// go. Required rather than defaulted: there is one engine now, and a
  /// default would be a quiet way to reintroduce a second.
  final CollectionSource source;
  final bool showCommands;

  /// How much to show. Defaults to [CollectionDetailLevel.full] because most
  /// callers (a standalone [CollectionScreen], the desktop pane) are already
  /// a dedicated space for one collection — collapsing has nothing to save
  /// there. Only an inline row in a list, which grows to hold this, has a
  /// reason to ask for less.
  final CollectionDetailLevel level;
  final bool showTitle;
  final Widget? inlineHeader;
  final Widget? inlineStatus;

  @override
  State<CollectionDetail> createState() => _CollectionDetailState();
}

/// A collection on its own screen, used on compact layouts.
class CollectionScreen extends StatelessWidget {
  const CollectionScreen({
    super.key,
    required this.collection,
    required this.source,
  });

  final Collection collection;
  final CollectionSource source;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppColors.surfaceDeep,
      body: SafeArea(
        child: PageBody(
          child: SingleChildScrollView(
            padding:
                const EdgeInsets.fromLTRB(kScreenGutter, 0, kScreenGutter, 24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                NavBackButton(onTap: () => Navigator.of(context).pop()),
                CollectionDetail(collection: collection, source: source),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _CollectionDetailState extends State<CollectionDetail> {
  bool _busy = false;

  Collection get _collection => widget.source.resolve(widget.collection);

  void _toast(String message, {ToastSeverity severity = ToastSeverity.info}) {
    if (mounted) showToast(context, message, severity: severity);
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } catch (error) {
      _toast('$error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _showInvite() async {
    final override = widget.source.showInvite;
    if (override != null) {
      await override(context, _collection);
      return;
    }

    final code = _collection.inviteCode;
    if (code == null) return;

    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppColors.surface,
        title: const Text('Invite a collaborator'),
        content: SizedBox(
          width: 260,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Share this code — anyone who enters it can join and add media.',
                style: AppText.secondary(color: AppColors.textDim),
              ),
              const SizedBox(height: 14),
              Center(
                child: Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Colors.white,
                    borderRadius: BorderRadius.circular(AppRadius.inner),
                  ),
                  child: QrImageView(
                    data: code,
                    version: QrVersions.auto,
                    size: 200,
                    backgroundColor: Colors.white,
                    eyeStyle: const QrEyeStyle(
                      eyeShape: QrEyeShape.square,
                      color: Colors.black,
                    ),
                    dataModuleStyle: const QrDataModuleStyle(
                      dataModuleShape: QrDataModuleShape.square,
                      color: Colors.black,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 14),
              SelectableText(code,
                  style: monoLabel(size: 12, letterSpacing: 0)),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () {
              Clipboard.setData(ClipboardData(text: code));
              showToast(
                dialogContext,
                'Invite code copied',
                severity: ToastSeverity.success,
              );
            },
            child: const Text('Copy'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Done'),
          ),
        ],
      ),
    );
  }

  Future<void> _addMedia() async {
    if (!supportsDirectPathSources) {
      if (!supportsNativeFilesSources && !supportsMobileGallerySources) {
        _toast(noCopySourceUnavailableMessage);
        return;
      }
    }
    final source = await showModalBottomSheet<String>(
      context: context,
      backgroundColor: AppColors.surface,
      builder: (sheetContext) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (supportsMobileGallerySources)
              ListTile(
                leading: const Icon(Icons.photo_library_outlined),
                title: const Text('Photos & videos'),
                onTap: () => Navigator.of(sheetContext).pop('photos'),
              ),
            if (supportsDirectPathSources || supportsNativeFilesSources)
              ListTile(
                leading: const Icon(Icons.folder_outlined),
                title: const Text('Files'),
                onTap: () => Navigator.of(sheetContext).pop('files'),
              ),
          ],
        ),
      ),
    );
    if (source == null || !mounted) return;

    List<PickedFile> picked = [];
    try {
      if (source == 'photos' && supportsMobileGallerySources) {
        picked = await PhotoLibraryPicker.pickMedia();
      } else if (source == 'files' && supportsNativeFilesSources) {
        picked = await NoCopySourcePicker.pickFiles();
      } else if (source == 'photos') {
        final files = await ImagePicker().pickMultipleMedia();
        picked = await Future.wait(
          files.map((file) => pickedFileFrom(
                name: file.name,
                nativePath: file.path,
              )),
        );
      } else {
        final result = await FilePicker.pickFiles(
          withData: false,
          allowMultiple: true,
          type: FileType.any,
        );
        picked = await Future.wait(
          (result?.files ?? []).map((file) => pickedFileFrom(
                name: file.name,
                nativePath: file.path,
              )),
        );
      }
    } catch (error) {
      _toast('Couldn\'t read those files: $error');
      return;
    }
    if (picked.isEmpty || !mounted) return;

    await _run(() async {
      final label =
          'Added ${DateTime.now().toIso8601String().split('T').first}';
      await widget.source.addMedia(_collection.id, label, picked);
      _toast('Preparing ${picked.length} item${picked.length == 1 ? '' : 's'}');
    });
  }

  Future<void> _fetchPending() => _run(() async {
        final started = await widget.source.fetchMedia(_collection.id);
        _toast('Fetching $started item${started == 1 ? '' : 's'}');
      });

  Future<void> _delete() async {
    final collection = _collection;
    final choice = await confirmCollectionDeletion(
      context,
      collectionName: collection.name,
    );
    if (choice == null || !mounted) return;
    // Not fire-and-forget: deleting genuinely fails (a torrent that isn't in
    // the session, a store write that can't land), and without this the
    // dialog would just close with nothing happening and no error shown.
    setState(() => _busy = true);
    try {
      await switch (choice) {
        CollectionDeletionChoice.collectionOnly =>
          widget.source.delete(collection.id),
        CollectionDeletionChoice.withFiles =>
          widget.source.deleteWithFiles(collection.id),
      };
      // Embedded, the list beside us simply drops it and the selection moves
      // on; there is no route to leave.
      if (mounted && Navigator.of(context).canPop()) {
        Navigator.of(context).pop();
      }
    } catch (error) {
      if (!mounted) return;
      showToast(context, "Couldn't delete this collection: $error");
      setState(() => _busy = false);
    }
  }

  Future<void> _openMedia(Collection collection, MediaItem media) async {
    final sourcePath = media.localPath;
    if (sourcePath?.startsWith('phasset://') ?? false) {
      try {
        await PhotoLibraryPicker.previewMedia(sourcePath!);
      } catch (error) {
        if (mounted) _toast('Couldn\'t preview ${media.label}: $error');
      }
      return;
    }
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => MediaViewerScreen(
          collection: collection,
          media: media,
          source: widget.source,
        ),
      ),
    );
  }

  @override
  void dispose() {
    widget.source.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.source.listenable,
      builder: (context, _) => _detail(_collection),
    );
  }

  Widget _detail(Collection collection) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        CollectionOverview(
          collection: collection,
          busy: _busy,
          onCommand: _command,
          history: widget.source.historyFor(collection.id),
          peerHistory: widget.source.peerHistoryFor(collection.id),
          showCommands: widget.showCommands,
          level: widget.level,
          showTitle: widget.showTitle,
          inlineHeader: widget.inlineHeader,
          inlineStatus: widget.inlineStatus,
          onInvite: _showInvite,
          onAddMedia: _addMedia,
          onFetch: _fetchPending,
        ),
        if (_busy)
          const Padding(
            padding: EdgeInsets.only(top: 10),
            child: LinearProgressIndicator(minHeight: 2),
          ),
        // Files are the deepest layer — worth the extra tap they cost a
        // merely-mid row, the same trade the peers section makes.
        if (widget.level == CollectionDetailLevel.full) ...[
          const SizedBox(height: 14),
          if (collection.media.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 22),
              child: Center(
                child: Text(
                  'Nothing in this collection yet.',
                  style: AppText.secondary(color: AppColors.textDim),
                ),
              ),
            )
          else
            ResizableMediaPreview(
              child: CollectionContents(
                collection: collection,
                onOpenMedia: (media) => _openMedia(collection, media),
              ),
            ),
        ],
      ],
    );
  }

  void _command(CollectionCommand command) {
    if (command == CollectionCommand.delete) {
      unawaited(_delete());
      return;
    }
    unawaited(_run(() async {
      final id = _collection.id;
      switch (command) {
        case CollectionCommand.restart:
          await widget.source.restart(id);
        case CollectionCommand.pause:
          await widget.source.pause(id);
        case CollectionCommand.delete:
          return;
      }
      _toast('${command.label} applied');
    }));
  }
}
