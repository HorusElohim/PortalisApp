import 'dart:async';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart' hide PickedFile;

import '../../../design/collection_deletion_dialog.dart';
import '../../../design/design.dart';
import '../../media/domain/item.dart';
import '../../media/presentation/viewer_screen.dart';
import '../domain/collection.dart';
import '../domain/picked_file.dart';
import '../platform/no_copy_source_picker.dart';
import '../platform/photo_library_picker.dart';
import '../platform/source_access.dart';
import 'add_sources.dart';
import 'contents.dart';
import 'commands.dart';
import 'overview.dart';
import 'share_qr.dart';
import 'source.dart';
import '../../../design/theme.dart';

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
    this.showTitle = true,
  });

  final Collection collection;

  /// Where this collection's live state comes from, and where its commands
  /// go. Required rather than defaulted: there is one engine now, and a
  /// default would be a quiet way to reintroduce a second.
  final CollectionSource source;
  final bool showCommands;
  final bool showTitle;

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
      // No reading column: a collection is a grid of media beside a chart and
      // a peer list, all of which reflow on their own. Centring it in a narrow
      // column left a wide window mostly empty beside content that could have
      // used it.
      body: SafeArea(
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
    );
  }
}

class _CollectionDetailState extends State<CollectionDetail> {
  bool _busy = false;

  /// A torrent draft has not started a transfer yet. Its checkbox changes stay
  /// here until the person explicitly presses Download; the engine's stored
  /// selection remains the initial all-selected file list in the meantime.
  Set<int>? _stagedDownloadEntries;

  /// Whether the collection is open for changes.
  ///
  /// `null` until the first build decides, because the answer depends on the
  /// collection: a draft opens in edit mode, since it exists only because
  /// somebody is in the middle of assembling it. Everything else opens closed
  /// and waits to be asked.
  bool? _editing;
  final _name = TextEditingController();

  bool get _isEditing => _editing ?? _collection.isDraft;

  @override
  void initState() {
    super.initState();
    // A draft opens in edit mode without anybody pressing anything, so its
    // suggested name has to already be in the field. Read from the resolved
    // collection rather than the seed: the seed is one frame stale.
    _name.text = _collection.name;
  }

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
        final files = await FilePicker.pickFiles(
          type: FileType.any,
        );
        picked = await Future.wait(
          files.map((file) => pickedFileFrom(
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

  Future<void> _shareQr() async {
    if (_busy) return;
    setState(() => _busy = true);
    String? uri;
    try {
      uri = await widget.source.shareUri(_collection.id);
    } catch (error) {
      if (mounted) _toast("Couldn't prepare a QR code: $error");
      return;
    } finally {
      if (mounted) setState(() => _busy = false);
    }
    if (!mounted) return;
    if (uri == null) {
      _toast('This collection is still preparing its share link');
      return;
    }
    await showCollectionShareQrDialog(
      context,
      collectionName: _collection.name,
      uri: uri,
    );
  }

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
    _name.dispose();
    // The source is not disposed here. Whoever constructed it owns it, and
    // some of them — the wide Home, which keeps one source for whichever row
    // is open — outlive any single detail. Disposing a borrowed source is how
    // an expanded row left the next one reading a dead subscription.
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.source.listenable,
      builder: (context, _) => _detail(_collection),
    );
  }

  /// Adds sources to this collection through the same sheet Home uses.
  ///
  /// A torrent chosen here is refused rather than silently starting a second
  /// download: this collection is what the person is adding to, and a magnet
  /// is not something that can be added to anything.
  Future<void> _addSources() async {
    final chosen = await showAddSourcesSheet(context);
    if (chosen == null || !mounted) return;
    if (chosen is! LocalSources) {
      _toast('A torrent becomes its own collection — add it from Home');
      return;
    }
    await _run(() => widget.source.addMedia(
          _collection.id,
          'Added ${DateTime.now().toIso8601String().substring(0, 10)}',
          chosen.files,
        ));
  }

  /// Renames only when the text actually changed, so leaving edit mode
  /// without touching the field never writes anything.
  Future<void> _commitName() async {
    final wanted = _name.text.trim();
    if (wanted.isEmpty || wanted == _collection.name) return;
    await _run(() => widget.source.rename(_collection.id, wanted));
  }

  Future<void> _share() async {
    await _commitName();
    if (!mounted) return;
    await _run(() => widget.source.publishDraft(_collection.id));
    if (mounted) {
      setState(() => _editing = false);
      _toast('Shared', severity: ToastSeverity.success);
    }
  }

  Future<void> _downloadSelected() async {
    final collection = _collection;
    final entries = _wantedEntries(collection);
    if (entries.isEmpty) {
      _toast('Waiting for this torrent\'s file list');
      return;
    }
    await _run(() => widget.source.setSelection(collection.id, entries));
    if (mounted) {
      setState(() {
        _editing = false;
        _stagedDownloadEntries = null;
      });
    }
  }

  void _toggleEditing(Collection collection) {
    if (_isEditing) {
      unawaited(_commitName());
      setState(() => _editing = false);
      return;
    }
    _name.text = collection.name;
    setState(() => _editing = true);
  }

  /// Whether edit mode is offering a name field for this collection.
  ///
  /// A torrent being added for the first time is not being named: somebody
  /// pasted a link to fetch what is in it, and the name belongs to the
  /// torrent. Its title stays on screen — it is the only thing saying what
  /// arrived — it simply is not a field. Re-open it later and it is theirs to
  /// rename like anything else.
  bool _namesInHeader(Collection collection) =>
      _isEditing && !(collection.isTorrent && collection.isDraft);

  Widget _detail(Collection collection) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (_isEditing)
          _EditHeader(
            name: _name,
            busy: _busy,
            autofocus: collection.isDraft,
            showName: _namesInHeader(collection),
            // Never for a torrent: its contents are fixed by its info hash,
            // so an Add here could only fail.
            onAdd: collection.isTorrent ? null : () => unawaited(_addSources()),
          ),
        CollectionOverview(
          collection: collection,
          busy: _busy,
          onCommand: _command,
          history: widget.source.historyFor(collection.id),
          peerHistory: widget.source.peerHistoryFor(collection.id),
          showCommands: widget.showCommands,
          // The title steps aside only when a field has replaced it. A
          // torrent that cannot be renamed still has to say what it is.
          showTitle: widget.showTitle && !_namesInHeader(collection),
          onAddMedia: _addMedia,
          onFetch: _fetchPending,
          onShareQr: collection.canShareQr ? () => unawaited(_shareQr()) : null,
          editing: _isEditing,
          paused: collection.isPaused,
        ),
        if (_busy)
          const Padding(
            padding: EdgeInsets.only(top: 10),
            child: LinearProgressIndicator(minHeight: 2),
          ),
        // Files are the deepest layer — worth the extra tap they cost a
        // merely-mid row, the same trade the peers section makes.
        ...[
          const SizedBox(height: 18),
          if (collection.media.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 22),
              child: Center(
                child: Text(
                  // An import with no file list yet is still being answered —
                  // by a descriptor on disk or by the swarm. Saying it holds
                  // nothing would be the screen guessing, and guessing wrong.
                  collection.isPreparing
                      ? 'Looking up what this torrent contains…'
                      : 'Nothing in this collection yet.',
                  style: AppText.secondary(color: AppColors.textDim),
                ),
              ),
            )
          else ...[
            // The grid sizes to its own content and the page scrolls, rather
            // than the grid scrolling inside a height a person had to drag to
            // set. A scroll region nested in a scroll region is the thing that
            // needed the handle; without it there is nothing to resize.
            SectionLabel('FILES - ${collection.media.length}'),
            const SizedBox(height: 8),
            CollectionContents(
              collection: collection,
              onOpenMedia: (media) => _openMedia(collection, media),
              stagedSelection: collection.isTorrent && collection.isDraft
                  ? _wantedEntries(collection)
                  : null,
              // Only while editing: a tap that changes what downloads is
              // not something a person should be able to do by brushing
              // past a tile they were only looking at.
              onToggleWanted: _isEditing && widget.source.supportsSelection
                  ? (media) => _toggleWanted(collection, media)
                  : null,
            ),
          ],
        ],
        if (_isEditing) ...[
          const SizedBox(height: 20),
          _EditFooter(
            busy: _busy,
            isDraft: collection.isDraft,
            isTorrent: collection.isTorrent,
            onShare: () => unawaited(_share()),
            onDownload: () => unawaited(_downloadSelected()),
            onDone: () => _toggleEditing(collection),
          ),
        ],
      ],
    );
  }

  /// Adds or removes one file from the selection screen.
  ///
  /// The whole selection is sent every time rather than a delta: the backend
  /// stores a set, and a delta would need this screen to agree about what it
  /// last saw. Before an imported torrent starts, the set remains local until
  /// Download confirms it; once moving, the same control updates the engine's
  /// live selection.
  void _toggleWanted(Collection collection, MediaItem media) {
    final entry = media.entryId;
    if (entry == null) return;
    final wanted = _wantedEntries(collection);
    if (!wanted.remove(entry)) wanted.add(entry);
    if (wanted.isEmpty) {
      _toast('Keep at least one file, or delete the collection');
      return;
    }
    if (collection.isTorrent && collection.isDraft) {
      setState(() => _stagedDownloadEntries = wanted);
      return;
    }
    unawaited(_run(() => widget.source.setSelection(collection.id, wanted)));
  }

  Set<int> _wantedEntries(Collection collection) {
    final staged = _stagedDownloadEntries;
    if (collection.isTorrent && collection.isDraft && staged != null) {
      return {...staged};
    }
    return {
      for (final media in collection.media)
        if (media.selected && media.entryId != null) media.entryId!,
    };
  }

  void _command(CollectionCommand command) {
    if (command == CollectionCommand.delete) {
      unawaited(_delete());
      return;
    }
    if (command == CollectionCommand.edit) {
      _toggleEditing(_collection);
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
        case CollectionCommand.edit:
          return;
      }
      _toast('${command.label} applied');
    }));
  }
}

/// What a collection is called, and how to put more in it.
///
/// At the top because it is what a person came here to set, and because the
/// name has to be legible while they look at what they are naming.
class _EditHeader extends StatelessWidget {
  const _EditHeader({
    required this.name,
    required this.busy,
    required this.autofocus,
    required this.showName,
    required this.onAdd,
  });

  final TextEditingController name;
  final bool busy;
  final bool autofocus;

  /// Whether there is a name here to give. A torrent arriving for the first
  /// time already has one, and it is not the person's to write.
  final bool showName;

  /// `null` where nothing can be added — a torrent's contents are its
  /// identity, so there is no such act.
  final VoidCallback? onAdd;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(bottom: 14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (showName) ...[
              Text('COLLECTION NAME', style: monoLabel(size: 10)),
              const SizedBox(height: 6),
              TextField(
                key: const Key('editCollectionName'),
                controller: name,
                autofocus: autofocus,
                enabled: !busy,
                textInputAction: TextInputAction.done,
                style: displayText(size: 18),
                decoration: InputDecoration(
                  isDense: true,
                  filled: true,
                  fillColor: AppColors.surfaceSunken,
                  enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppRadius.inner),
                    borderSide: BorderSide(color: AppColors.border),
                  ),
                  focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(AppRadius.inner),
                    borderSide: BorderSide(color: AppColors.signal),
                  ),
                ),
              ),
              const SizedBox(height: 10),
            ],
            if (onAdd != null)
              PrimaryActionButton(
                key: const Key('editAddSources'),
                label: 'Add photos, files or a folder',
                icon: Icons.add,
                expand: true,
                tone: ActionButtonTone.neutral,
                onTap: busy ? null : onAdd,
              ),
          ],
        ),
      );
}

/// The one irreversible thing edit mode does, at the end of the page.
///
/// Last because sharing is a decision about everything above it: the name,
/// the files, and which of them are wanted. A button that sits before all of
/// that asks for a commitment to something the person has not read yet.
class _EditFooter extends StatelessWidget {
  const _EditFooter({
    required this.busy,
    required this.isDraft,
    required this.isTorrent,
    required this.onShare,
    required this.onDownload,
    required this.onDone,
  });

  final bool busy;

  /// A native draft has never been shared, so its finishing move is "Share".
  /// A torrent draft instead receives somebody else's selected files; an
  /// already-settled collection is only being edited, so its move is "Done".
  final bool isDraft;
  final bool isTorrent;
  final VoidCallback onShare;
  final VoidCallback onDownload;
  final VoidCallback onDone;

  @override
  Widget build(BuildContext context) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PrimaryActionButton(
            key: const Key('editFinish'),
            label: isDraft
                ? (isTorrent
                    ? 'Download selected files'
                    : 'Share this collection')
                : 'Done',
            icon: isDraft
                ? (isTorrent ? Icons.download : Icons.ios_share)
                : Icons.check,
            expand: true,
            tone: isDraft ? ActionButtonTone.ember : ActionButtonTone.neutral,
            onTap: busy
                ? null
                : (isDraft ? (isTorrent ? onDownload : onShare) : onDone),
          ),
          if (isDraft) ...[
            const SizedBox(height: 8),
            Text(
              isTorrent
                  ? 'This device will receive the selected files.'
                  : 'Nothing has left this device yet.',
              textAlign: TextAlign.center,
              style: monoLabel(size: 10),
            ),
          ],
        ],
      );
}
