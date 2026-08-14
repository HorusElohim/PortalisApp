import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../../media/domain/media_item.dart';
import '../../media/presentation/media_grid.dart';
import '../domain/collection.dart';
import 'collection_presentation.dart';

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
  Widget build(BuildContext context) {
    final entries = collection.entries;
    if (!collection.isShared || entries.length <= 1) {
      return MediaGrid(
        media: collection.media,
        color: collection.hue,
        onOpenMedia: onOpenMedia,
        onToggleWanted: onToggleWanted,
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final entry in entries) ...[
          _EntryHeader(collection: collection, entry: entry),
          const SizedBox(height: 8),
          MediaGrid(
            color: collection.hue,
            media: entry.media,
            onOpenMedia: onOpenMedia,
            onToggleWanted: onToggleWanted,
          ),
          const SizedBox(height: 18),
        ],
      ],
    );
  }
}

class _EntryHeader extends StatelessWidget {
  const _EntryHeader({required this.collection, required this.entry});

  final Collection collection;
  final CollectionEntry entry;

  @override
  Widget build(BuildContext context) {
    final author = entry.addedBy == null
        ? null
        : collection.collaborators
            .where((item) => item.deviceId == entry.addedBy)
            .firstOrNull;
    final facts = <String>[
      if (entry.fetched) plural(entry.media.length, 'file'),
      if (entry.totalBytes > 0) formatBytes(entry.totalBytes),
      if (author != null) 'from ${author.name}',
    ];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                entry.label,
                overflow: TextOverflow.ellipsis,
                style: displayText(size: 13.5),
              ),
            ),
            if (!entry.fetched) ...[
              const SizedBox(width: 8),
              StatusBadge(label: 'NOT FETCHED'),
            ],
          ],
        ),
        if (facts.isNotEmpty) ...[
          const SizedBox(height: 2),
          Text(
            facts.join(' · '),
            overflow: TextOverflow.ellipsis,
            style: monoLabel(size: 10, letterSpacing: 0.2),
          ),
        ],
        if (entry.fetched && entry.totalBytes > 0 && entry.progress < 1.0) ...[
          const SizedBox(height: 7),
          ClipRRect(
            borderRadius: BorderRadius.circular(AppRadius.pill),
            child: LinearProgressIndicator(
              value: entry.progress,
              minHeight: 3,
              backgroundColor: AppColors.borderStrong,
              valueColor: AlwaysStoppedAnimation(collection.hue),
            ),
          ),
        ],
      ],
    );
  }
}
