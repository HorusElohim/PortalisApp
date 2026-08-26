import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/collections/platform/security_scoped_sources.dart';

/// A sandboxed Mac app loses access to a chosen file when the process that
/// chose it exits. Portalis seeds the person's original file rather than a
/// copy, so that access has to be retained explicitly or a restart cannot read
/// what it is still sharing.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const channel = MethodChannel('app.portalis/security-scoped-sources');
  final calls = <MethodCall>[];

  void answerWith(Object? Function(MethodCall call) handler) {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      return handler(call);
    });
  }

  setUp(calls.clear);

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);
    debugDefaultTargetPlatformOverride = null;
  });

  test('macOS retains access to every chosen source', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    answerWith((call) => ['/Users/mark/Movies/episode.mov']);

    final retained = await SecurityScopedSources.retain(
      ['/Users/mark/Movies/episode.mov'],
    );

    expect(retained, ['/Users/mark/Movies/episode.mov']);
    expect(calls.single.method, 'retain');
    expect(
      (calls.single.arguments as Map)['paths'],
      ['/Users/mark/Movies/episode.mov'],
    );
  });

  /// A file the platform will not bookmark is left out rather than reported as
  /// shareable: claiming it here would become a failure to seed after the next
  /// restart, which is much harder to explain.
  test('a source the platform refuses to retain is not reported as retained',
      () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    answerWith((call) => const <String>[]);

    expect(await SecurityScopedSources.retain(['/Volumes/ejected/clip.mov']),
        isEmpty);
  });

  /// The channel is the sandbox's, not Portalis'. Linux and Windows are not
  /// sandboxed this way and must not pay for a call that cannot apply.
  test('platforms without a sandbox scope never reach the channel', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    answerWith((call) => const <String>[]);

    expect(
      await SecurityScopedSources.retain(['/home/mark/clip.mov']),
      ['/home/mark/clip.mov'],
      reason:
          'an unsandboxed path is already readable, so it is already retained',
    );
    expect(calls, isEmpty);
  });

  /// An older host binary without the channel must not break selection. The
  /// pick still works for this run; only persistence across a restart is lost.
  test('a host without the channel still allows the selection', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);

    expect(
      await SecurityScopedSources.retain(['/Users/mark/Movies/episode.mov']),
      ['/Users/mark/Movies/episode.mov'],
    );
  });

  test('releasing gives up access for files no longer seeded', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    answerWith((call) => null);

    await SecurityScopedSources.release(['/Users/mark/Movies/episode.mov']);

    expect(calls.single.method, 'release');
  });

  test('an empty selection is never a platform call', () async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    answerWith((call) => const <String>[]);

    await SecurityScopedSources.retain(const []);
    await SecurityScopedSources.release(const []);

    expect(calls, isEmpty);
  });
}
