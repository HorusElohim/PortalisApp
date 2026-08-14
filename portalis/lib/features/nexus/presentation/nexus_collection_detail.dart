import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../application/nexus_app_controller.dart';
import '../domain/nexus_app_state.dart';

/// Detail projection for a Nexus collection that is not waiting for torrent
/// selection. Mutations stay on the command boundary; this screen never
/// reconstructs collection state locally.
class NexusCollectionDetail extends StatefulWidget {
  const NexusCollectionDetail({
    super.key,
    required this.collection,
    required this.controller,
  });

  final int collection;
  final NexusAppController controller;

  @override
  State<NexusCollectionDetail> createState() => _NexusCollectionDetailState();
}

class _NexusCollectionDetailState extends State<NexusCollectionDetail> {
  bool _busy = false;

  Future<void> _rename(NexusCollection collection) async {
    final name = await promptForText(
      context,
      title: 'Rename collection',
      initialValue: collection.name,
      confirmLabel: 'Rename',
    );
    if (name == null || name == collection.name || !mounted) return;
    await _send(
      NexusCommand(
        kind: 'renameCollection',
        collection: collection.id,
        name: name,
      ),
    );
  }

  Future<void> _delete(NexusCollection collection) async {
    final confirmed = await confirmAction(
      context,
      title: 'Delete "${collection.name}"?',
      message: 'The collection will be removed from this device. Downloaded '
          'files stay on disk.',
      confirmLabel: 'Delete collection',
      destructive: true,
    );
    if (!confirmed || !mounted) return;
    await _send(
      NexusCommand(
        kind: 'deleteCollection',
        collection: collection.id,
        deleteFiles: false,
      ),
      closeAfterSuccess: true,
    );
  }

  Future<void> _send(
    NexusCommand command, {
    bool closeAfterSuccess = false,
  }) async {
    setState(() => _busy = true);
    try {
      await widget.controller.send(command);
      if (closeAfterSuccess && mounted) Navigator.of(context).maybePop();
    } catch (error) {
      if (mounted) showToast(context, '$error', severity: ToastSeverity.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.controller,
        builder: (context, _) {
          final current = widget.controller.state?.collections
              .where((item) => item.id == widget.collection)
              .firstOrNull;
          if (current == null) {
            return const AppScreen(
              title: 'Collection',
              body: Center(
                  child: Text('This collection is no longer available.')),
            );
          }
          return AppScreen(
            title: current.name,
            subtitle: Text(
              '${plural(current.entries, 'file')} · '
              '${formatBytes(current.totalBytes.toInt())}',
            ),
            onBack: () => Navigator.of(context).maybePop(),
            footer: _actions(current),
            body: StreamBuilder<NexusDetail?>(
              stream: widget.controller.watchDetail(widget.collection),
              builder: (context, snapshot) {
                final detail = snapshot.data;
                if (detail == null) {
                  return Center(
                    child: Text(
                      'Nexus has no per-file descriptor for this collection yet.',
                      textAlign: TextAlign.center,
                      style: AppText.body(color: AppColors.textDim),
                    ),
                  );
                }
                return ListView.separated(
                  padding: const EdgeInsets.fromLTRB(
                    kScreenGutter,
                    0,
                    kScreenGutter,
                    28,
                  ),
                  itemCount: detail.entries.length,
                  separatorBuilder: (_, __) => const SizedBox(height: 10),
                  itemBuilder: (_, index) {
                    final entry = detail.entries[index];
                    return SurfaceCard(
                      child: Row(
                        children: [
                          Icon(
                            entry.available
                                ? Icons.check_circle_outline
                                : Icons.description_outlined,
                            color: entry.available
                                ? AppColors.signal
                                : AppColors.textDim,
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Text(
                              entry.label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: AppText.body(),
                            ),
                          ),
                          Text(
                            formatBytes(entry.bytes.toInt()),
                            style: monoLabel(
                              size: 10,
                              color: AppColors.textDim,
                            ),
                          ),
                        ],
                      ),
                    );
                  },
                );
              },
            ),
          );
        },
      );

  Widget _actions(NexusCollection collection) => Row(
        children: [
          Expanded(
            child: OutlineActionButton(
              key: const Key('nexusRenameCollection'),
              label: 'Rename',
              icon: Icons.edit_outlined,
              expand: true,
              onTap: _busy ? null : () => _rename(collection),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: TextButton.icon(
              key: const Key('nexusDeleteCollection'),
              onPressed: _busy ? null : () => _delete(collection),
              icon: const Icon(Icons.delete_outline),
              label: const Text('Delete'),
              style: TextButton.styleFrom(foregroundColor: AppColors.danger),
            ),
          ),
        ],
      );
}
