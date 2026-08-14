import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../domain/collection.dart';
import 'collection_commands.dart';
import 'collection_views.dart';

/// The wide-layout collection list. The caller supplies optional detail so
/// this component does not depend on a navigation strategy.
class CollectionsList extends StatelessWidget {
  const CollectionsList({
    super.key,
    required this.collections,
    required this.openId,
    required this.onOpen,
    required this.detailFor,
    required this.onCommand,
    this.canExpand,
    this.footer,
  });

  final List<Collection> collections;
  final String? openId;
  final ValueChanged<Collection> onOpen;
  final Widget Function(
    Collection collection,
    CollectionDetailLevel level,
    Widget inlineHeader,
    Widget inlineStatus,
  ) detailFor;
  final ValueChanged<(Collection, CollectionCommand)> onCommand;

  /// Whether a collection may grow into its own detail at all. `null` (the
  /// default) means every one of them can, which is every caller this had
  /// until one collection stopped being "settled content to view" and became
  /// "a choice still to make" — nothing to grow a row into, only a screen to
  /// go to. `false` gives that row [CollectionRow]'s own older, simpler
  /// behaviour: a plain tap, and the command bar always showing.
  final bool Function(Collection collection)? canExpand;

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
          final isOpen = collection.id == openId;
          final expandable = canExpand?.call(collection) ?? true;
          return CollectionRow(
            collection: collection,
            selected: isOpen,
            onTap: () => onOpen(collection),
            detail: expandable
                ? (level, inlineHeader, inlineStatus) =>
                    detailFor(collection, level, inlineHeader, inlineStatus)
                : null,
            onCommand: (command) => onCommand((collection, command)),
          );
        },
      );
}
