import 'dart:async';
import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import 'package:portalis/features/peer_hints/services/qr_peer_hint_service.dart';

/// Widget for scanning QR codes to obtain peer hints for magnetic xpe.
///
/// This widget provides a full-screen QR scanner that extracts magnet links
/// containing x.pe parameters for peer-to-peer connection bootstrapping.
class QrPeerHintScanner extends StatefulWidget {
  /// Callback when a magnet link with peer hints is scanned
  final Function(String magnetLink) onMagnetScanned;

  /// Optional title for the scanner screen
  final String? title;

  const QrPeerHintScanner({
    super.key,
    required this.onMagnetScanned,
    this.title,
  });

  @override
  State<QrPeerHintScanner> createState() => _QrPeerHintScannerState();
}

class _QrPeerHintScannerState extends State<QrPeerHintScanner> {
  final QrPeerHintService _qrService = QrPeerHintService();
  StreamSubscription<String>? _scanningSubscription;
  bool _isInitialized = false;

  @override
  void initState() {
    super.initState();
    _initializeScanner();
  }

  @override
  void dispose() {
    _scanningSubscription?.cancel();
    _qrService.stopScanning();
    super.dispose();
  }

  Future<void> _initializeScanner() async {
    if (!mounted) return;
    setState(() => _isInitialized = true);

    final stream = _qrService.startScanning();
    _scanningSubscription = stream.listen(
      (magnetLink) {
        // The callback pops this route. Let route disposal stop and dispose the
        // controller; disposing it here leaves MobileScanner building for one
        // frame with an already-disposed controller.
        _scanningSubscription?.cancel();

        // Notify the callback
        widget.onMagnetScanned(magnetLink);

        // Show success feedback
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text('Portalis collection scanned successfully!'),
              backgroundColor: Colors.green,
            ),
          );
        }
      },
      onError: (error) {
        debugPrint('QR scanning error: $error');
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(
              content: Text('QR scanning error: $error'),
              backgroundColor: Colors.red,
            ),
          );
        }
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    if (!_isInitialized) {
      return const Scaffold(
        body: Center(
          child: CircularProgressIndicator(),
        ),
      );
    }

    return Scaffold(
      appBar: AppBar(
        title: Text(widget.title ?? 'Scan Peer Hint QR Code'),
        leading: IconButton(
          icon: const Icon(Icons.close),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ),
      body: Stack(
        children: [
          // QR Scanner View
          MobileScanner(
            controller: _qrService.controller!,
            onDetect: (barcodeCapture) {
              // Handling is done via the stream in initState
            },
          ),

          // Scan area overlay
          Container(
            color: Colors.black54,
            child: CustomPaint(
              painter: _QRScannerPainter(),
            ),
          ),

          // Instructions
          Positioned(
            bottom: 24,
            left: 0,
            right: 0,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text(
                  'Point camera at a shared Portalis collection QR code',
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    color: Colors.white,
                    fontSize: 16,
                  ),
                ),
                const SizedBox(height: 8),
                const Text(
                  'Format: portalis://import?magnet=…',
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    color: Colors.white70,
                    fontSize: 14,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _QRScannerPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final Paint paint = Paint()
      ..color = Colors.white.withValues(alpha: 0.3)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2;

    // Draw scan rectangle in the center
    final double scanSize = size.shortestSide * 0.7;
    final double offset = (size.width - scanSize) / 2;
    final Rect scanRect = Rect.fromLTWH(
      offset,
      (size.height - scanSize) / 2,
      scanSize,
      scanSize,
    );

    canvas.drawRect(scanRect, paint);

    // Draw corner markers
    final double markerSize = 24.0;
    final Paint markerPaint = Paint()
      ..color = Colors.white
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3.0;

    // Top-left
    canvas.drawLine(
      Offset(scanRect.left, scanRect.top + markerSize),
      Offset(scanRect.left, scanRect.top),
      markerPaint,
    );
    canvas.drawLine(
      Offset(scanRect.left, scanRect.top),
      Offset(scanRect.left + markerSize, scanRect.top),
      markerPaint,
    );

    // Top-right
    canvas.drawLine(
      Offset(scanRect.right - markerSize, scanRect.top),
      Offset(scanRect.right, scanRect.top),
      markerPaint,
    );
    canvas.drawLine(
      Offset(scanRect.right, scanRect.top),
      Offset(scanRect.right, scanRect.top + markerSize),
      markerPaint,
    );

    // Bottom-left
    canvas.drawLine(
      Offset(scanRect.left, scanRect.bottom - markerSize),
      Offset(scanRect.left, scanRect.bottom),
      markerPaint,
    );
    canvas.drawLine(
      Offset(scanRect.left, scanRect.bottom),
      Offset(scanRect.left + markerSize, scanRect.bottom),
      markerPaint,
    );

    // Bottom-right
    canvas.drawLine(
      Offset(scanRect.right - markerSize, scanRect.bottom),
      Offset(scanRect.right, scanRect.bottom),
      markerPaint,
    );
    canvas.drawLine(
      Offset(scanRect.right, scanRect.bottom),
      Offset(scanRect.right, scanRect.bottom - markerSize),
      markerPaint,
    );
  }

  @override
  bool shouldRepaint(covariant _QRScannerPainter oldDelegate) => false;
}
