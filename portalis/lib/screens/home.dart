import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';

import '../app/app_controllers.dart';
import '../design/design.dart';
import '../features/collections/domain/picked_file.dart';
import '../features/collections/presentation/collection_join.dart';
import '../features/collections/presentation/collection_share.dart';
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
    this.onShare,
    this.onJoin,
  });

  final bool embedded;
  final void Function([List<PickedFile>? initialFiles])? onShare;
  final ValueChanged<String>? onJoin;

  @override
  State<Home> createState() => _HomeState();
}

class _HomeState extends State<Home> {
  String _query = '';
  NexusCollectionFilter _filter = NexusCollectionFilter.all;
  bool _dropBusy = false;

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
    final screen = collection.status == 'Preparing'
        ? NexusTorrentPreparation(
            collection: collection.id,
            controller: AppControllers.nexusApp,
          )
        : NexusCollectionDetail(
            collection: collection.id,
            controller: AppControllers.nexusApp,
          );
    _push(screen);
  }

  Future<void> _createCollection() async {
    final name = await promptForText(
      context,
      title: 'Create collection',
      hint: 'Collection name',
      confirmLabel: 'Create',
    );
    if (name == null || name.isEmpty || !mounted) return;
    try {
      final accepted = await AppControllers.nexusApp.send(
        NexusCommand(kind: 'createCollection', name: name),
      );
      final collection = accepted.collection;
      if (collection == null) {
        throw StateError('Nexus did not identify the new collection');
      }
      if (!mounted) return;
      _push(NexusCollectionDetail(
        collection: collection,
        controller: AppControllers.nexusApp,
      ));
    } catch (error) {
      if (mounted) showToast(context, '$error', severity: ToastSeverity.error);
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
          filter: _filter,
          onOpen: _openCollection,
          onSearch: (query) => setState(() => _query = query),
          onFilterChanged: (filter) => setState(() => _filter = filter),
          onJoin: _openJoin,
          onImportTorrent: _importTorrent,
          onCreateCollection: _createCollection,
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
