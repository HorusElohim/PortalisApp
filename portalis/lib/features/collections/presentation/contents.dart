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
  });

  final Collection collection;
  final ValueChanged<MediaItem> onOpenMedia;

  /// Passed straight to the grids. `null` where the collection's files are
  /// not a choice — see [MediaGrid.onToggleWanted].
  final ValueChanged<MediaItem>? onToggleWanted;

  @override
  Widget build(BuildContext context) => MediaGrid(
        media: collection.media,
        color: collection.hue,
        onOpenMedia: onOpenMedia,
        onToggleWanted: onToggleWanted,
      );
}
