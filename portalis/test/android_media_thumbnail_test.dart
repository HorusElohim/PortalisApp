import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/media/domain/item.dart';
import 'package:portalis/features/media/platform/heic_preview.dart';
import 'package:portalis/features/media/presentation/thumbnail.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const channel = MethodChannel('app.portalis/heic-preview');
  final calls = <MethodCall>[];

  setUp(() {
    debugDefaultTargetPlatformOverride = TargetPlatform.android;
    calls.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      return Uint8List.fromList(_onePixelPng);
    });
  });

  tearDown(() {
    debugDefaultTargetPlatformOverride = null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);
  });

  test('native previews reuse one decode for the same source', () async {
    await Future.wait([
      HeicPreview.decode('content://media/external/images/media/cache-9', 160),
      HeicPreview.decode('content://media/external/images/media/cache-9', 160),
    ]);

    expect(calls, hasLength(1));
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('Android content image asks the native zero-copy preview adapter',
      (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: SizedBox(
          width: 84,
          height: 84,
          child: MediaThumbnail(
            media: MediaItem(
              label: 'camera.jpg',
              sizeBytes: 42,
              fetched: true,
              localPath: 'content://media/external/images/media/7',
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(calls, hasLength(1));
    expect(calls.single.method, 'decode');
    expect(
      calls.single.arguments,
      containsPair('path', 'content://media/external/images/media/7'),
    );
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('Android content video asks the native zero-copy preview adapter',
      (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: SizedBox(
          width: 84,
          height: 84,
          child: MediaThumbnail(
            media: MediaItem(
              label: 'camera.mp4',
              sizeBytes: 42,
              fetched: true,
              localPath: 'content://media/external/video/media/8',
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(calls, hasLength(1));
    expect(calls.single.method, 'decode');
    expect(
      calls.single.arguments,
      containsPair('path', 'content://media/external/video/media/8'),
    );
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets('Android filesystem video uses a frame instead of a player',
      (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: SizedBox(
          width: 84,
          height: 84,
          child: MediaThumbnail(
            media: MediaItem(
              label: 'received.mov',
              sizeBytes: 42,
              fetched: true,
              localPath: '/data/user/0/com.portalis/files/received.mov',
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(calls, hasLength(1));
    expect(calls.single.method, 'decode');
    debugDefaultTargetPlatformOverride = null;
  });
}

const _onePixelPng = <int>[
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
  0x00,
  0x00,
  0x00,
  0x0d,
  0x49,
  0x48,
  0x44,
  0x52,
  0x00,
  0x00,
  0x00,
  0x01,
  0x00,
  0x00,
  0x00,
  0x01,
  0x08,
  0x04,
  0x00,
  0x00,
  0x00,
  0xb5,
  0x1c,
  0x0c,
  0x02,
  0x00,
  0x00,
  0x00,
  0x0b,
  0x49,
  0x44,
  0x41,
  0x54,
  0x78,
  0xda,
  0x63,
  0xfc,
  0xff,
  0x1f,
  0x00,
  0x03,
  0x03,
  0x02,
  0x00,
  0xef,
  0xbf,
  0x68,
  0x7f,
  0x00,
  0x00,
  0x00,
  0x00,
  0x49,
  0x45,
  0x4e,
  0x44,
  0xae,
  0x42,
  0x60,
  0x82,
];
