import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../domain/media_item.dart';
import 'media_piece_frame.dart';
import 'media_thumbnail.dart';

/// A thumbnail grid of media, capped so a tile stays thumbnail-sized at any
/// window width.
class MediaGrid extends StatelessWidget {
  const MediaGrid({
    super.key,
    required this.media,
    required this.color,
    required this.onOpenMedia,
  });

  final List<MediaItem> media;

  /// The collection's colour, used by the piece frame. Passed in rather than
  /// derived so this grid needs nothing but the media it draws — which is what
  /// lets a Nexus collection and a legacy one share it.
  final Color color;
  final ValueChanged<MediaItem> onOpenMedia;

  @override
  Widget build(BuildContext context) => GridView.builder(
        shrinkWrap: true,
        physics: const NeverScrollableScrollPhysics(),
        // A fixed column count still made previews enormous on a wide Home
        // card. Cap each tile instead, keeping the media and its piece frame
        // deliberately thumbnail-sized at every window width.
        gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
          maxCrossAxisExtent: 84,
          mainAxisSpacing: 7,
          crossAxisSpacing: 6,
          childAspectRatio: 0.78,
        ),
        itemCount: media.length,
        itemBuilder: (context, index) {
          final item = media[index];
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
                            MediaThumbnail(media: item, borderRadius: 6),
                            if (!item.fetched)
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
                style: AppText.caption(color: AppColors.text, height: 1.1),
              ),
              Text(
                !item.fetched
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
