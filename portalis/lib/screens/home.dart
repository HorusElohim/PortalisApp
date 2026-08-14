import 'dart:async';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';

import '../app/app_controllers.dart';
import '../design/collection_deletion_dialog.dart';
import '../design/design.dart';
import '../features/collections/domain/picked_file.dart';
import '../features/collections/presentation/collection_commands.dart';
import '../features/collections/presentation/collection_join.dart';
import '../features/collections/presentation/collection_share.dart';
import '../features/nexus/data/nexus_collection_source.dart';
import '../features/nexus/domain/nexus_app_state.dart';
import '../features/nexus/presentation/nexus_collection_detail.dart';
import '../features/nexus/presentation/nexus_home_library.dart';
import '../features/nexus/presentation/nexus_torrent_preparation.dart';

/// App-shell adapter for the Nexus collection library. It coordinates routes
/// and desktop file drops; collection state stays in the Nexus feature.
class Home extends StatefulWidget {
  const Home({
    super.key,
    this.embedded = false,
    this.openId,
    this.onOpen,
    this.onShare,
    this.onJoin,
  });

  final bool embedded;

  /// The one collection grown into its own detail in the wide list. `null`
  /// on the compact layout, which has nowhere to grow a row into.
  final int? openId;

  /// Supplied by the wide shell, which owns [openId] and toggles it in place
  /// of navigating. `null` on the compact layout, where opening a collection
  /// falls back to pushing its own screen — the same split
  /// [AdaptiveShellState.openCollection] always made.
  final ValueChanged<int>? onOpen;
  final void Function([List<PickedFile>? initialFiles])? onShare;
  final ValueChanged<String>? onJoin;

  @override
  State<Home> createState() => _HomeState();
}

class _HomeState extends State<Home> {
  String _query = '';
  bool _dropBusy = false;

  // Feeds the currently-open row's inline `CollectionDetail`. Owned here,
  // one at a time, because Nexus's detail stream costs nothing until
  // something asks for it — an inline-expanded row is exactly one such ask,
  // and there is at most one open at a time.
  NexusCollectionSource? _openSource;

  @override
  void initState() {
    super.initState();
    _syncOpenSource();
  }

  @override
  void didUpdateWidget(Home oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.openId != oldWidget.openId) _syncOpenSource();
  }

  void _syncOpenSource() {
    _openSource?.dispose();
    _openSource = widget.openId == null
        ? null
        : NexusCollectionSource(
            controller: AppControllers.nexusApp,
            collectionId: widget.openId!,
          );
  }

  @override
  void dispose() {
    _openSource?.dispose();
    super.dispose();
  }

  void _push(Widget screen) {
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => screen));
  }

  void _openShare([List<PickedFile>? initialFiles]) {
    if (widget.onShare != null) {
      widget.onShare!(initialFiles);
    } else {
      _push(ShareScreen(initialFiles: initialFiles));
    }
  }

  void _openJoin(String code) {
    if (widget.onJoin != null) {
      widget.onJoin!(code);
    } else {
      _push(JoinCollectionScreen(initialCode: code));
    }
  }

  void _openCollection(NexusCollection collection) {
    // Every collection but one defers to `onOpen` when the wide shell
    // supplied one; see `nexusCollectionNeedsSelection`.
    if (widget.onOpen != null && !nexusCollectionNeedsSelection(collection)) {
      widget.onOpen!(collection.id);
      return;
    }
    _push(nexusCollectionScreen(collection, AppControllers.nexusApp));
  }

  void _handleCommand((NexusCollection, CollectionCommand) action) {
    final (collection, command) = action;
    if (command == CollectionCommand.delete) {
      unawaited(_delete(collection));
      return;
    }
    unawaited(_runCommand(collection, command));
  }

  Future<void> _runCommand(
    NexusCollection collection,
    CollectionCommand command,
  ) async {
    try {
      switch (command) {
        case CollectionCommand.restart:
          await sendSetPaused(AppControllers.nexusApp, collection.id, paused: false);
        case CollectionCommand.pause:
          await sendSetPaused(AppControllers.nexusApp, collection.id, paused: true);
        case CollectionCommand.delete:
          return;
      }
      if (mounted) showToast(context, '${command.label} applied');
    } catch (error) {
      if (mounted) {
        showToast(context, '$error', severity: ToastSeverity.error);
      }
    }
  }

  Future<void> _delete(NexusCollection collection) async {
    final choice = await confirmCollectionDeletion(
      context,
      collectionName: collection.name,
    );
    if (choice == null || !mounted) return;
    try {
      await sendDeleteCollection(
        AppControllers.nexusApp,
        collection.id,
        deleteFiles: choice == CollectionDeletionChoice.withFiles,
      );
      // Embedded, the list simply drops it and the selection moves on;
      // there is no route to leave.
    } catch (error) {
      if (mounted) {
        showToast(
          context,
          "Couldn't delete this collection: $error",
          severity: ToastSeverity.error,
        );
      }
    }
  }

  Future<void> _handleDrop(DropDoneDetails details) async {
    final files = details.files;
    if (files.isEmpty) return;

    if (files.length == 1 &&
        files.single.name.toLowerCase().endsWith('.torrent')) {
      setState(() => _dropBusy = true);
      try {
        await _importTorrent(files.single.path);
        if (mounted) {
          showToast(
            context,
            'Torrent prepared — choose files next',
            severity: ToastSeverity.success,
          );
        }
      } catch (error) {
        if (mounted) {
          showToast(
            context,
            'Couldn\'t add .torrent file: $error',
            severity: ToastSeverity.error,
          );
        }
      } finally {
        if (mounted) setState(() => _dropBusy = false);
      }
      return;
    }

    final picked = await Future.wait(files.map((file) => pickedFileFrom(
          name: file.name,
          nativePath: file.path,
        )));
    if (mounted) _openShare(picked);
  }

  Future<void> _importTorrent(String source) async {
    final accepted = await AppControllers.nexusApp.send(
      NexusCommand.importTorrent(source),
    );
    final collection = accepted.collection;
    if (collection == null) {
      throw StateError('Nexus did not identify the imported torrent');
    }
    // Magnet metadata is not resolved by the current substrate yet, so there
    // is no real file selection to present. Local .torrent imports resolve
    // their descriptor immediately and can enter the preparation flow.
    if (!source.toLowerCase().endsWith('.torrent')) return;
    if (!mounted) return;
    _push(NexusTorrentPreparation(
      collection: collection,
      controller: AppControllers.nexusApp,
    ));
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: AppControllers.nexusApp,
      builder: (context, _) {
        final library = NexusHomeLibrary(
          wide: widget.embedded,
          state: AppControllers.nexusApp.state,
          error: AppControllers.nexusApp.lastError,
          query: _query,
          openId: widget.openId,
          openSource: _openSource,
          onOpen: _openCollection,
          onCommand: _handleCommand,
          onSearch: (query) => setState(() => _query = query),
          onJoin: _openJoin,
          onImportTorrent: _importTorrent,
          onCreateCollection: () => _openShare(),
        );
        if (!widget.embedded) return library;
        return DropTarget(
          onDragDone: _handleDrop,
          child: _dropBusy
              ? Stack(
                  children: [
                    library,
                    const Positioned(
                      top: 0,
                      left: 0,
                      right: 0,
                      child: LinearProgressIndicator(minHeight: 2),
                    ),
                  ],
                )
              : library,
        );
      },
    );
  }
}
