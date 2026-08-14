import 'package:flutter/material.dart';

import 'theme.dart';

enum CollectionDeletionChoice { collectionOnly, withFiles }

/// Asks whether downloaded files should go too, then hands back the choice.
///
/// The dialog only, deliberately: what happens for each choice is a
/// collection-source decision (the legacy controller for a torrent-backed
/// collection, a Nexus command for one backed by the core), and this widget
/// has no opinion about which. Extracted so both sources show the exact same
/// prompt rather than two dialogs that could drift.
Future<CollectionDeletionChoice?> confirmCollectionDeletion(
  BuildContext context, {
  required String collectionName,
}) => showDialog<CollectionDeletionChoice>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppColors.surface,
        title: Text('Delete "$collectionName"?'),
        content: Text(
          'The collection will be deleted from this device. Choose whether '
          'its downloaded files stay on disk or are deleted too. Other '
          'collaborators keep their own copies.',
          style: AppText.secondary(color: AppColors.textDim),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel'),
          ),
          TextButton(
            key: const Key('deleteCollectionOnly'),
            onPressed: () => Navigator.of(dialogContext)
                .pop(CollectionDeletionChoice.collectionOnly),
            child: Text('Delete', style: TextStyle(color: AppColors.danger)),
          ),
          TextButton(
            key: const Key('deleteCollectionWithFiles'),
            onPressed: () => Navigator.of(dialogContext)
                .pop(CollectionDeletionChoice.withFiles),
            child: Text(
              'Delete with files',
              style: TextStyle(color: AppColors.danger),
            ),
          ),
        ],
      ),
    );
