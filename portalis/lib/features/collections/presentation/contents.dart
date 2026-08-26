import 'package:flutter/material.dart';

import '../../media/domain/item.dart';
import '../../media/presentation/grid.dart';
import '../domain/collection.dart';
import 'peer_color.dart';

/// Manifest-entry grouping and media tiles for a collection.
class CollectionContents extends StatelessWidget {
  const CollectionContents({
    super.key,
    required this.collection,
    required this.onOpenMedia,
    this.onToggleWanted,
    this.stagedSelection,
  });

  final Collection collection;
  final ValueChanged<MediaItem> onOpenMedia;

  /// Passed straight to the grids. `null` where the collection's files are
  /// not a choice — see [MediaGrid.onToggleWanted].
  final ValueChanged<MediaItem>? onToggleWanted;

  /// A draft torrent's unconfirmed checkbox state. It is deliberately a
  /// presentation-only overlay; the source is not told until Download.
  final Set<int>? stagedSelection;

  @override
  Widget build(BuildContext context) => MediaGrid(
        media: [
          for (final item in collection.media)
            if (stagedSelection != null && item.entryId != null)
              item.withSelected(stagedSelection!.contains(item.entryId))
            else
              item,
        ],
        color: collection.hue,
        onOpenMedia: onOpenMedia,
        onToggleWanted: onToggleWanted,
      );
}
