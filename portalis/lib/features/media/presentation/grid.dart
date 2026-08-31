import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../domain/item.dart';
import 'piece_frame.dart';
import 'thumbnail.dart';

/// A thumbnail grid of media, capped so a tile stays thumbnail-sized at any
/// window width.
class MediaGrid extends StatelessWidget {
  const MediaGrid({
    super.key,
    required this.media,
    required this.color,
    required this.onOpenMedia,
    this.onToggleWanted,
  });

  final List<MediaItem> media;

  /// The collection's colour, used by the piece frame. Passed in rather than
  /// derived so this grid needs nothing but the media it draws — which is what
  /// lets a Nexus collection and a legacy one share it.
  final Color color;
  final ValueChanged<MediaItem> onOpenMedia;

  /// Toggles whether a file is one the collection should fetch.
  ///
  /// `null` where nothing can be chosen, which is most collections — a tile
  /// then draws no toggle at all rather than a disabled one. Its own control
  /// rather than the tile's tap, so choosing what to download and opening
  /// what has downloaded stay two different gestures.
  final ValueChanged<MediaItem>? onToggleWanted;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
        builder: (context, constraints) => _grid(constraints.maxWidth),
      );

  /// Tiles stay thumbnail-sized on a phone-width layout, but a card that has
  /// the room to show them larger should use it rather than tiling the same
  /// small squares across the extra width.
  Widget _grid(double width) => GridView.builder(
        shrinkWrap: true,
        physics: const NeverScrollableScrollPhysics(),
        // A fixed column count still made previews enormous on a wide Home
        // card. Cap each tile instead, keeping the media and its piece frame
        // deliberately thumbnail-sized at phone width, and modestly larger
        // once there is width to spare.
        gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(
          maxCrossAxisExtent: width >= 640 ? 112 : 84,
          mainAxisSpacing: 7,
          crossAxisSpacing: 6,
          childAspectRatio: 0.78,
        ),
        itemCount: media.length,
        itemBuilder: (context, index) {
          final item = media[index];
          final choosable = onToggleWanted != null && item.entryId != null;
          // Only a file nobody asked for is dimmed. Fading anything else would
          // make an ordinary collection look half-disabled.
          final skipped = choosable && !item.selected;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: MediaPieceFrame(
                  media: item,
                  color: color,
                  borderRadius: BorderRadius.circular(AppRadius.tight),
                  child: Container(
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(AppRadius.tight),
                      border: Border.all(color: AppColors.border),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: Material(
                      color: Colors.transparent,
                      child: InkWell(
                        onTap: () => onOpenMedia(item),
                        child: Stack(
                          fit: StackFit.expand,
                          children: [
                            AnimatedOpacity(
                              duration: const Duration(milliseconds: 160),
                              opacity: skipped ? 0.3 : 1,
                              child:
                                  MediaThumbnail(media: item, borderRadius: 6),
                            ),
                            if (!item.fetched && !skipped)
                              Container(
                                color: AppColors.surfaceDeep
                                    .withValues(alpha: 0.55),
                                alignment: Alignment.center,
                                child: Icon(
                                  Icons.cloud_download_outlined,
                                  size: 22,
                                  color: AppColors.signalSoft,
                                ),
                              ),
                            if (choosable)
                              Positioned(
                                top: 3,
                                right: 3,
                                child: _WantedToggle(
                                  key: Key('mediaWanted:${item.entryId}'),
                                  wanted: item.selected,
                                  color: color,
                                  onTap: () => onToggleWanted!(item),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 5),
              Text(
                item.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: AppText.caption(
                  color: skipped ? AppColors.textGhost : AppColors.text,
                  height: 1.1,
                ),
              ),
              Text(
                skipped
                    ? 'skipped'
                    : !item.fetched
                        ? 'not fetched'
                        : item.progress < 1.0
                            ? formatProgressPercent(item.progress)
                            : item.sizeBytes > 0
                                ? formatBytes(item.sizeBytes)
                                : '',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: monoLabel(size: 9.5, letterSpacing: 0.2),
              ),
            ],
          );
        },
      );
}

/// The corner control that says whether a file is wanted.
///
/// Filled with the collection's own colour when it is, hollow when it is not,
/// so a glance at the grid says what will be fetched without reading a word.
class _WantedToggle extends StatelessWidget {
  const _WantedToggle({
    super.key,
    required this.wanted,
    required this.color,
    required this.onTap,
  });

  final bool wanted;
  final Color color;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
        checked: wanted,
        button: true,
        label: wanted ? 'Downloading this file' : 'Skipping this file',
        child: GestureDetector(
          onTap: onTap,
          // The dot is small; the target around it is not.
          behavior: HitTestBehavior.opaque,
          child: Padding(
            padding: const EdgeInsets.all(4),
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 160),
              width: 17,
              height: 17,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: wanted
                    ? color
                    : AppColors.surfaceDeep.withValues(alpha: 0.72),
                border: Border.all(
                  color: wanted ? color : AppColors.borderStrong,
                  width: 1.2,
                ),
              ),
              child: wanted
                  ? Icon(Icons.check, size: 11, color: AppColors.surfaceDeep)
                  : null,
            ),
          ),
        ),
      );
}
