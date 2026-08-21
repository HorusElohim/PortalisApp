import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/collections/presentation/contents.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
  testWidgets('collection contents renders generated AppEntry values directly',
      (tester) async {
    final collection = AppCollection(
      id: 1,
      name: 'Iceland trip',
      nature: 'Native',
      role: 'Owner',
      revision: BigInt.one,
      status: 'Available',
      members: Uint32List(0),
      entries: 1,
      totalBytes: BigInt.from(1000),
      onDiskBytes: BigInt.from(400),
      uploadedBytes: BigInt.zero,
    );
    final detail = AppDetail(
      id: collection.id,
      entries: [
        AppEntry(
          id: 42,
          label: 'clip.mp4',
          bytes: BigInt.from(1000),
          downloadedBytes: BigInt.from(400),
          selected: true,
          available: false,
        ),
      ],
      pieces: Uint8List(0),
      peers: const [],
    );

    await tester.pumpWidget(
      MaterialApp(
        home: CollectionContents(
          collection: collection,
          detail: detail,
          onOpenMedia: (_) {},
        ),
      ),
    );

    expect(find.text('clip.mp4'), findsOneWidget);
    expect(find.text('40%'), findsOneWidget);
  });
}
