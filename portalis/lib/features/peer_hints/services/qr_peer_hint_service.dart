import 'package:flutter/foundation.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../../../app/collection_link.dart';
import '../../collections/domain/paste.dart';

/// Service for scanning QR codes to extract peer hints from magnet links.
///
/// This service uses the mobile_scanner package to scan QR codes and
/// extract magnet links containing x.pe parameters for peer-to-peer
/// connection bootstrapping.
class QrPeerHintService {
  /// Singleton instance
  static final QrPeerHintService _instance = QrPeerHintService._internal();

  factory QrPeerHintService() => _instance;

  QrPeerHintService._internal();

  /// Scanner controller
  MobileScannerController? _controller;

  /// Expose the controller for the MobileScanner widget
  MobileScannerController? get controller => _controller;

  /// Whether the scanner is currently active
  bool get isScanning => _controller != null;

  /// Start scanning for QR codes containing magnet links with peer hints
  ///
  /// Returns a Stream that emits decoded magnet strings when QR codes are scanned
  Stream<String> startScanning() {
    _controller = MobileScannerController(
      facing: defaultTargetPlatform == TargetPlatform.macOS
          ? CameraFacing.front
          : CameraFacing.back,
      formats: [BarcodeFormat.qrCode],
      detectionSpeed: DetectionSpeed.normal,
      returnImage: false,
    );

    return _controller!.barcodes
        .map((barcodeCapture) {
          if (barcodeCapture.barcodes.isEmpty) return null;
          final barcodeValue = barcodeCapture.barcodes.first.rawValue;
          if (barcodeValue == null) return null;
          if (barcodeValue.startsWith('magnet:')) return barcodeValue;
          if (looksLikeInvitation(barcodeValue)) return barcodeValue;
          final uri = Uri.tryParse(barcodeValue);
          return uri == null ? null : collectionMagnetFromLink(uri);
        })
        .where((value) => value != null)
        .cast<String>();
  }

  /// Stop the QR code scanner
  Future<void> stopScanning() async {
    await _controller?.stop();
    _controller?.dispose();
    _controller = null;
  }

  /// Check if scanning is available on this platform
  static Future<bool> get isAvailable async {
    try {
      final controller = MobileScannerController();
      await controller.start();
      await controller.stop();
      return true;
    } catch (e) {
      return false;
    }
  }
}
