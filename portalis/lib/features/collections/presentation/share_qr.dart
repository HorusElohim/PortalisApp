import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../../../app/collection_link.dart';
import '../../../design/theme.dart';

/// Shows the existing import URI for a carried collection as an in-person QR.
///
/// The URI comes from Nexus rather than a process-local collection handle: a
/// second device can import a magnet URI, while an app handle would name
/// nothing outside the current runtime.
Future<void> showCollectionShareQrDialog(
  BuildContext context, {
  required String collectionName,
  required String uri,
}) =>
    showDialog<void>(
      context: context,
      builder: (dialogContext) => Dialog(
        key: const Key('collectionShareQrDialog'),
        backgroundColor: AppColors.surface,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 360),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'Scan to import $collectionName',
                  textAlign: TextAlign.center,
                  style: displayText(size: 20, weight: FontWeight.w700),
                ),
                const SizedBox(height: 10),
                Text(
                  'This QR contains the collection\'s magnet link.',
                  textAlign: TextAlign.center,
                  style: AppText.secondary(color: AppColors.textDim),
                ),
                const SizedBox(height: 20),
                DecoratedBox(
                  decoration: BoxDecoration(
                    color: Colors.white,
                    borderRadius: BorderRadius.circular(16),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.all(16),
                    child: CollectionShareQrCode(
                      key: const Key('collectionShareQrCode'),
                      uri: collectionShareLink(uri),
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                TextButton(
                  key: const Key('collectionShareQrClose'),
                  onPressed: () => Navigator.of(dialogContext).pop(),
                  child: const Text('Done'),
                ),
              ],
            ),
          ),
        ),
      ),
    );

/// Renders one app-routable collection import URI as a QR code.
class CollectionShareQrCode extends StatelessWidget {
  const CollectionShareQrCode({super.key, required this.uri});

  final String uri;

  @override
  Widget build(BuildContext context) => QrImageView(data: uri, size: 240);
}
