import 'dart:async';

import 'package:flutter/material.dart';

import '../../../design/collection_deletion_dialog.dart';
import '../../../design/design.dart';
import '../../../design/resizable_media_preview.dart';
import '../../../design/theme.dart';
import '../../../nexus/application/app_controller.dart';
import '../../../nexus/application/nexus_gateway.dart';
import '../../../nexus/domain/app_state.dart';
import '../domain/picked_file.dart';
import '../platform/photo_library_picker.dart';
import '../../media/presentation/viewer_screen.dart';
import 'add_sources.dart';
import 'commands.dart';
import 'contents.dart';
import 'overview.dart';

/// Renders one generated collection summary with its selected detail stream.
class CollectionDetail extends StatefulWidget {
  const CollectionDetail({
    super.key,
    required this.collection,
    required this.detail,
    required this.readings,
    required this.contacts,
    required this.controller,
    this.showCommands = true,
    this.showTitle = true,
  });

  final AppCollection collection;
  final AppDetail? detail;
  final List<Reading> readings;
  final List<AppContact> contacts;
  final AppController controller;
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
    required this.detail,
    required this.readings,
    required this.contacts,
    required this.controller,
  });

  final AppCollection collection;
  final AppDetail? detail;
  final List<Reading> readings;
  final List<AppContact> contacts;
  final AppController controller;

  @override
  Widget build(BuildContext context) => Scaffold(
        backgroundColor: AppColors.surfaceDeep,
        body: SafeArea(
          child: SingleChildScrollView(
            padding:
                const EdgeInsets.fromLTRB(kScreenGutter, 0, kScreenGutter, 24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                NavBackButton(onTap: () => Navigator.of(context).pop()),
                CollectionDetail(
                  collection: collection,
                  detail: detail,
                  readings: readings,
                  contacts: contacts,
                  controller: controller,
                ),
              ],
            ),
          ),
        ),
      );
}

class _CollectionDetailState extends State<CollectionDetail> {
  bool _busy = false;
  bool? _editing;
  final _name = TextEditingController();

  AppCollection get _collection => widget.collection;
  AppDetail? get _detail => widget.detail;
  bool get _isEditing => _editing ?? _collection.isDraft;
  bool get _supportsSelection => _collection.isTorrent;

  @override
  void initState() {
    super.initState();
    _name.text = _collection.name;
  }

  @override
  void didUpdateWidget(covariant CollectionDetail oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_isEditing && oldWidget.collection.name != _collection.name) {
      _name.text = _collection.name;
    }
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  void _toast(String message, {ToastSeverity severity = ToastSeverity.info}) {
    if (mounted) showToast(context, message, severity: severity);
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } catch (error) {
      _toast('$error', severity: ToastSeverity.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _sendMedia(String label, List<PickedFile> files) =>
      _run(() async {
        await widget.controller.send(appCommand(
          kind: 'addMedia',
          collection: _collection.id,
          label: label,
          files: [
            for (final file in files)
              AppSourceFile(
                name: file.name,
                path: file.path,
                bytes: BigInt.from(file.lengthBytes),
              ),
          ],
        ));
        _toast('Preparing ${files.length} item${files.length == 1 ? '' : 's'}');
      });

  Future<void> _addSources() async {
    final chosen = await showAddSourcesSheet(context);
    if (chosen == null || !mounted) return;
    if (chosen is! LocalSources) {
      _toast('A torrent becomes its own collection — add it from Home');
      return;
    }
    await _sendMedia(
      'Added ${DateTime.now().toIso8601String().substring(0, 10)}',
      chosen.files,
    );
  }

  Future<void> _delete() async {
    final choice = await confirmCollectionDeletion(
      context,
      collectionName: _collection.name,
    );
    if (choice == null || !mounted) return;
    await _run(() async {
      await widget.controller.send(appCommand(
        kind: 'deleteCollection',
        collection: _collection.id,
        deleteFiles: choice == CollectionDeletionChoice.withFiles,
      ));
      if (mounted && Navigator.of(context).canPop()) {
        Navigator.of(context).pop();
      }
    });
  }

  Future<void> _openMedia(AppEntry entry) async {
    final path = entry.localPath;
    if (path?.startsWith('phasset://') ?? false) {
      try {
        await PhotoLibraryPicker.previewMedia(path!);
      } catch (error) {
        _toast('Couldn\'t preview ${entry.label}: $error',
            severity: ToastSeverity.error);
      }
      return;
    }
    if (!mounted) return;
    await Navigator.of(context).push(MaterialPageRoute(
      builder: (_) => MediaViewerScreen(
        controller: widget.controller,
        collectionId: _collection.id,
        entryId: entry.id,
      ),
    ));
  }

  Future<void> _commitName() async {
    final wanted = _name.text.trim();
    if (wanted.isEmpty || wanted == _collection.name) return;
    await _run(() => widget.controller.send(appCommand(
          kind: 'renameCollection',
          collection: _collection.id,
          name: wanted,
        )));
  }

  Future<void> _share() async {
    await _commitName();
    if (!mounted) return;
    await _run(() => widget.controller.send(
          appCommand(kind: 'publishDraft', collection: _collection.id),
        ));
    if (mounted) {
      setState(() => _editing = false);
      _toast('Shared', severity: ToastSeverity.success);
    }
  }

  void _toggleEditing() {
    if (_isEditing) {
      unawaited(_commitName());
      setState(() => _editing = false);
      return;
    }
    _name.text = _collection.name;
    setState(() => _editing = true);
  }

  bool get _namesInHeader =>
      _isEditing && !(_collection.isTorrent && _collection.isDraft);

  void _toggleWanted(AppEntry entry) {
    final wanted = {
      for (final item in _detail?.entries ?? const <AppEntry>[])
        if (item.selected) item.id,
    };
    if (!wanted.remove(entry.id)) wanted.add(entry.id);
    if (wanted.isEmpty) {
      _toast('Keep at least one file, or delete the collection');
      return;
    }
    unawaited(_run(() => widget.controller.send(appCommand(
          kind: 'downloadSelection',
          collection: _collection.id,
          entries: wanted.toList()..sort(),
        ))));
  }

  void _command(CollectionCommand command) {
    if (command == CollectionCommand.delete) {
      unawaited(_delete());
      return;
    }
    if (command == CollectionCommand.edit) {
      _toggleEditing();
      return;
    }
    final paused = command == CollectionCommand.pause;
    unawaited(_run(() => widget.controller.send(appCommand(
          kind: 'setPaused',
          collection: _collection.id,
          paused: paused,
        ))));
  }

  @override
  Widget build(BuildContext context) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (_isEditing)
            _EditHeader(
              name: _name,
              busy: _busy,
              autofocus: _collection.isDraft,
              showName: _namesInHeader,
              onAdd:
                  _collection.isTorrent ? null : () => unawaited(_addSources()),
            ),
          CollectionOverview(
            collection: _collection,
            detail: _detail,
            readings: widget.readings,
            contacts: widget.contacts,
            busy: _busy,
            onCommand: _command,
            showCommands: widget.showCommands,
            showTitle: widget.showTitle && !_namesInHeader,
            editing: _isEditing,
            paused: _collection.isPaused,
          ),
          if (_busy)
            const Padding(
              padding: EdgeInsets.only(top: 10),
              child: LinearProgressIndicator(minHeight: 2),
            ),
          const SizedBox(height: 14),
          if ((_detail?.entries ?? const <AppEntry>[]).isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 22),
              child: Center(
                child: Text(
                  _collection.status == 'Importing'
                      ? 'Looking up what this torrent contains…'
                      : 'Nothing in this collection yet.',
                  style: AppText.secondary(color: AppColors.textDim),
                ),
              ),
            )
          else
            ResizableMediaPreview(
              child: CollectionContents(
                collection: _collection,
                detail: _detail,
                onOpenMedia: (entry) => unawaited(_openMedia(entry)),
                onToggleWanted:
                    _isEditing && _supportsSelection ? _toggleWanted : null,
              ),
            ),
          if (_isEditing) ...[
            const SizedBox(height: 20),
            _EditFooter(
              busy: _busy,
              isDraft: _collection.isDraft,
              onShare: () => unawaited(_share()),
              onDone: _toggleEditing,
            ),
          ],
        ],
      );
}

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
  final bool showName;
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

class _EditFooter extends StatelessWidget {
  const _EditFooter({
    required this.busy,
    required this.isDraft,
    required this.onShare,
    required this.onDone,
  });

  final bool busy;
  final bool isDraft;
  final VoidCallback onShare;
  final VoidCallback onDone;

  @override
  Widget build(BuildContext context) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PrimaryActionButton(
            key: const Key('editFinish'),
            label: isDraft ? 'Share this collection' : 'Done',
            icon: isDraft ? Icons.ios_share : Icons.check,
            expand: true,
            tone: isDraft ? ActionButtonTone.ember : ActionButtonTone.neutral,
            onTap: busy ? null : (isDraft ? onShare : onDone),
          ),
          if (isDraft) ...[
            const SizedBox(height: 8),
            Text(
              'Nothing has left this device yet.',
              textAlign: TextAlign.center,
              style: monoLabel(size: 10),
            ),
          ],
        ],
      );
}
