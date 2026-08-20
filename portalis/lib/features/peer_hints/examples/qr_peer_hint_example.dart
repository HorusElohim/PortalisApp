import 'package:flutter/material.dart';
import 'package:portalis/features/peer_hints/widgets/qr_peer_hint_scanner.dart';

/// Example screen showing how to use the QR peer hint scanner
class QrPeerHintExampleScreen extends StatefulWidget {
  const QrPeerHintExampleScreen({Key? key}) : super(key: key);
  
  @override
  State<QrPeerHintExampleScreen> createState() => _QrPeerHintExampleScreenState();
}

class _QrPeerHintExampleScreenState extends State<QrPeerHintExampleScreen> {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('QR Peer Hint Example'),
      ),
      body: Center(
        child: ElevatedButton.icon(
          onPressed: () async {
            final result = await Navigator.of(context).push<String>(
              MaterialPageRoute(
                builder: (context) => QrPeerHintScanner(
                  onMagnetScanned: (magnetLink) {
                    // Handle the scanned magnet link
                    // This could be passed to the torrent import function
                    if (mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(
                          content: Text('Scanned: $magnetLink'),
                          duration: const Duration(seconds: 3),
                        ),
                      );
                    }
                    
                    // Return the result to close the scanner
                    Navigator.of(context).pop(magnetLink);
                  },
                  title: 'Scan Peer Hint QR Code',
                ),
              ),
            );
            
            if (result != null && mounted) {
              // Process the scanned magnet link here
              // For example, pass it to the torrent import function
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: Text('Received magnet link: $result'),
                  backgroundColor: Colors.info,
                ),
              );
            }
          },
          icon: const Icon(Icons.qr_code_scanner),
          label: const Text('Scan QR Code for Peer Hints'),
        ),
      ),
    );
  }
}