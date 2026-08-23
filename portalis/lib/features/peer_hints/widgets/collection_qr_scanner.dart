import 'package:flutter/material.dart';

import 'qr_peer_hint_scanner.dart';

/// Opens the camera and returns the validated magnet from a Portalis QR code.
///
/// The scanner accepts both the app-owned `portalis://import` URI and a raw
/// magnet for compatibility with older shared QR codes. Validation remains in
/// [QrPeerHintScanner] and [collectionMagnetFromLink].
Future<String?> scanCollectionQrCode(BuildContext context) {
  return Navigator.of(context).push<String>(
    MaterialPageRoute(
      builder: (scannerContext) => QrPeerHintScanner(
        title: 'Scan collection QR code',
        onMagnetScanned: (magnet) =>
            Navigator.of(scannerContext).pop<String>(magnet),
      ),
    ),
  );
}
