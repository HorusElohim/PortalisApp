import 'package:flutter/material.dart';

import '../../media/domain/item.dart';
import '../../media/presentation/grid.dart';
import '../../media/presentation/piece_frame.dart';
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
  Widget build(BuildContext context) {
    final media = collection.media;
    final staged = [
      for (final item in media)
        if (stagedSelection != null && item.entryId != null)
          item.withSelected(stagedSelection!.contains(item.entryId))
        else
          item,
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (media.any((item) => item.progressBuckets.length == 16)) ...[
          ProgressLegend(color: collection.hue),
          const SizedBox(height: 8),
        ],
        MediaGrid(
          media: staged,
          color: collection.hue,
          onOpenMedia: onOpenMedia,
          onToggleWanted: onToggleWanted,
        ),
      ],
    );
  }
}
