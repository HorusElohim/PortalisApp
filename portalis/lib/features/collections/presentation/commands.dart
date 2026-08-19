import 'package:flutter/material.dart';

import '../../../design/design.dart';

/// Lifecycle and destructive actions reserved for a collection command API.
enum CollectionCommand { restart, pause, edit, delete }

extension CollectionCommandPresentation on CollectionCommand {
  String get label => switch (this) {
        CollectionCommand.restart => 'Start',
        CollectionCommand.pause => 'Pause',
        CollectionCommand.edit => 'Edit',
        CollectionCommand.delete => 'Delete',
      };

  IconData get icon => switch (this) {
        CollectionCommand.restart => Icons.play_arrow_outlined,
        CollectionCommand.pause => Icons.pause_outlined,
        CollectionCommand.edit => Icons.edit_outlined,
        CollectionCommand.delete => Icons.delete_outline,
      };

  String get tooltip => switch (this) {
        CollectionCommand.restart => 'Start transferring',
        CollectionCommand.pause => 'Stop transferring',
        CollectionCommand.edit => 'Rename, add files, choose what to fetch',
        CollectionCommand.delete => 'Delete collection',
      };
}

/// One reusable command strip for both compact and wide collection previews.
/// Every command maps to a native collection lifecycle operation.
class CollectionCommandBar extends StatelessWidget {
  const CollectionCommandBar({
    super.key,
    required this.busy,
    required this.onCommand,
    this.paused = false,
    this.editing = false,
    this.showEdit = true,
    this.trailingActions = const [],
  });

  final bool busy;
  final ValueChanged<CollectionCommand> onCommand;

  /// Which half of the start/stop pair to offer.
  ///
  /// One button, not two. A paused collection has nothing to pause and a
  /// running one has nothing to start, so showing both meant one of them was
  /// always a no-op dressed as an action.
  final bool paused;
  final bool editing;
  final bool showEdit;
  final List<Widget> trailingActions;

  List<CollectionCommand> get _commands => [
        paused ? CollectionCommand.restart : CollectionCommand.pause,
        if (showEdit) CollectionCommand.edit,
        CollectionCommand.delete,
      ];

  @override
  Widget build(BuildContext context) => Wrap(
        spacing: 10,
        runSpacing: 10,
        children: [
          for (final command in _commands)
            OutlineActionButton(
              key: Key('collectionCommand${command.name}'),
              label: command.label,
              icon: command.icon,
              tone: switch (command) {
                CollectionCommand.delete => ActionButtonTone.ember,
                // Lit while it is on, so leaving edit mode is visibly the
                // same control as entering it.
                CollectionCommand.edit when editing => ActionButtonTone.signal,
                _ => ActionButtonTone.neutral,
              },
              tooltip: command.tooltip,
              compact: true,
              onTap: busy ? null : () => onCommand(command),
            ),
          ...trailingActions,
        ],
      );
}
