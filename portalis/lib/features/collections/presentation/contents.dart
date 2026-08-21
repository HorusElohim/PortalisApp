import 'package:flutter/material.dart';

import '../../../nexus/domain/app_state.dart';
import '../../media/presentation/grid.dart';

/// Manifest entries for the selected generated collection detail.
class CollectionContents extends StatelessWidget {
  const CollectionContents({
    super.key,
    required this.collection,
    required this.detail,
    required this.onOpenMedia,
    this.onToggleWanted,
  });

  final AppCollection collection;
  final AppDetail? detail;
  final ValueChanged<AppEntry> onOpenMedia;
  final ValueChanged<AppEntry>? onToggleWanted;

  @override
  Widget build(BuildContext context) => MediaGrid(
        entries: detail?.entries ?? const [],
        color: collection.hue,
        onOpenMedia: onOpenMedia,
        onToggleWanted: onToggleWanted,
      );
}
