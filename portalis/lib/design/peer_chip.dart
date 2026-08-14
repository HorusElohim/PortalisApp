import 'package:flutter/material.dart';

import 'design.dart';
import '../theme.dart';

/// One peer, named or anonymous.
///
/// Plain values only — a label, an optional leading widget, an optional
/// detail, a colour. A peer surface is the same surface whether the peer came
/// from a signed membership or from a swarm address, and the difference
/// between those two is carried by what a caller puts in it rather than by
/// which screen is drawing it.
class PeerChip extends StatelessWidget {
  PeerChip({
    super.key,
    required this.label,
    this.leading,
    this.detail,
    Color? color,
  }) : color = color ?? AppColors.textDim;

  final String label;
  final Widget? leading;
  final String? detail;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final maxWidth = MediaQuery.sizeOf(context).width - 2 * kScreenGutter;
    final colorful = color != AppColors.textDim;
    return ConstrainedBox(
      constraints: BoxConstraints(maxWidth: maxWidth),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
        decoration: BoxDecoration(
          color: colorful
              ? color.withValues(
                  alpha: color == AppColors.ember ? 0.12 : 0.08,
                )
              : AppColors.surface,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(
            color: colorful
                ? color.withValues(
                    alpha: color == AppColors.ember ? 0.35 : 0.24,
                  )
                : AppColors.border,
          ),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (leading != null) ...[leading!, const SizedBox(width: 6)],
                Flexible(
                  child: Text(
                    label,
                    overflow: TextOverflow.ellipsis,
                    style:
                        monoLabel(size: 10.5, color: color, letterSpacing: 0),
                  ),
                ),
                if (detail != null) ...[
                  const SizedBox(width: 6),
                  Flexible(
                    child: Text(
                      detail!,
                      overflow: TextOverflow.ellipsis,
                      style: monoLabel(
                        size: 9,
                        color: AppColors.textDim,
                        letterSpacing: 0,
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }
}
