import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/collections/presentation/peers.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
  testWidgets('peer surface renders generated swarm addresses directly',
      (tester) async {
    final collection = AppCollection(
      id: 1,
      name: 'Some torrent',
      nature: 'Torrent',
      role: 'Owner',
      revision: BigInt.one,
      status: 'Downloading',
      members: Uint32List(0),
      entries: 1,
      totalBytes: BigInt.from(1000),
      onDiskBytes: BigInt.from(250),
      uploadedBytes: BigInt.zero,
      transfer: const AppTransfer(
        progress: .25,
        downBytesPerSecond: 2,
        upBytesPerSecond: 0,
        peers: 1,
        etaSecs: null,
      ),
    );
    final detail = AppDetail(
      id: 1,
      entries: [],
      pieces: Uint8List(0),
      peers: ['203.0.113.5:6881'],
    );

    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: CollectionPeers(
          collection: collection,
          detail: detail,
          contacts: const [],
        ),
      ),
    ));

    expect(find.text('203.0.113.5:6881'), findsOneWidget);
    expect(find.text('25%'), findsOneWidget);
    expect(find.text('250 B of 1 KB received on this device'), findsOneWidget);
  });
}
