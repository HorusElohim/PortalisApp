import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../application/nexus_app_controller.dart';
import '../domain/nexus_app_state.dart';

/// Read-only detail projection for a Nexus collection that is not waiting for
/// torrent selection. Mutations deliberately stay on the command boundary;
/// this screen never reconstructs collection state locally.
class NexusCollectionDetail extends StatelessWidget {
  const NexusCollectionDetail({
    super.key,
    required this.collection,
    required this.controller,
  });

  final int collection;
  final NexusAppController controller;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: controller,
        builder: (context, _) {
          final current = controller.state?.collections
              .where((item) => item.id == collection)
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
            body: StreamBuilder<NexusDetail?>(
              stream: controller.watchDetail(collection),
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
}
