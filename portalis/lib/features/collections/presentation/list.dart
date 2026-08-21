import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../nexus/domain/app_state.dart';
import 'commands.dart';
import 'views.dart';

/// The wide-layout collection list. The caller supplies optional detail so
/// this component does not depend on a navigation strategy.
class CollectionsList extends StatelessWidget {
  const CollectionsList({
    super.key,
    required this.collections,
    required this.onOpen,
    required this.onCommand,
    this.footer,
  });

  final List<AppCollection> collections;
  final ValueChanged<AppCollection> onOpen;
  final ValueChanged<(AppCollection, CollectionCommand)> onCommand;

  final Widget? footer;

  @override
  Widget build(BuildContext context) => ListView.separated(
        padding: const EdgeInsets.fromLTRB(
          kScreenGutter + 8,
          0,
          kScreenGutter + 8,
          28,
        ),
        itemCount: collections.length + (footer == null ? 0 : 1),
        separatorBuilder: (_, __) => const SizedBox(height: 14),
        itemBuilder: (context, index) {
          if (index == collections.length) {
            return Padding(
              padding: const EdgeInsets.only(top: 4, bottom: 12),
              child: footer,
            );
          }
          final collection = collections[index];
          // A row is a row on every layout now. Growing one into its own
          // detail was the wide window's private idea of "open", and it left
          // the collection's own controls reachable on one layout and not the
          // other — see `AdaptiveShellState.openCollection`.
          return CollectionRow(
            collection: collection,
            onTap: () => onOpen(collection),
            onCommand: (command) => onCommand((collection, command)),
          );
        },
      );
}
