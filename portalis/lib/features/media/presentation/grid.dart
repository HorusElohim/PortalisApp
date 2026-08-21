import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';
import 'piece_frame.dart';
import 'thumbnail.dart';

/// A thumbnail grid over generated entries in the selected detail projection.
class MediaGrid extends StatelessWidget {
  const MediaGrid({
    super.key,
    required this.entries,
    required this.color,
    required this.onOpenMedia,
    this.onToggleWanted,
  });

  final List<AppEntry> entries;
  final Color color;
  final ValueChanged<AppEntry> onOpenMedia;
  final ValueChanged<AppEntry>? onToggleWanted;

  @override
  Widget build(BuildContext context) => GridView.builder(
        shrinkWrap: true,
        physics: const NeverScrollableScrollPhysics(),
        gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
          maxCrossAxisExtent: 84,
          mainAxisSpacing: 7,
          crossAxisSpacing: 6,
          childAspectRatio: 0.78,
        ),
        itemCount: entries.length,
        itemBuilder: (context, index) {
          final entry = entries[index];
          final choosable = onToggleWanted != null;
          final skipped = choosable && !entry.selected;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: MediaPieceFrame(
                  media: entry,
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
                        onTap: () => onOpenMedia(entry),
                        child: Stack(
                          fit: StackFit.expand,
                          children: [
                            AnimatedOpacity(
                              duration: const Duration(milliseconds: 160),
                              opacity: skipped ? 0.3 : 1,
                              child:
                                  MediaThumbnail(media: entry, borderRadius: 6),
                            ),
                            if (!entry.fetched && !skipped)
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
                                  key: Key('mediaWanted:${entry.id}'),
                                  wanted: entry.selected,
                                  color: color,
                                  onTap: () => onToggleWanted!(entry),
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
                entry.label,
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
                    : !entry.fetched
                        ? 'not fetched'
                        : entry.progress < 1.0
                            ? formatProgressPercent(entry.progress)
                            : entry.sizeBytes > 0
                                ? formatBytes(entry.sizeBytes)
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
