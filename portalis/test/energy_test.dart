import 'test_support.dart';

AppCollection _c({
  String status = 'Available',
  int down = 0,
  int up = 0,
  int livePeers = 0,
  int entries = 1,
}) =>
    buildNexusCollection(
      name: 'Trip',
      status: status,
      entries: entries,
      transfer: down > 0 || up > 0 || livePeers > 0
          ? AppTransfer(
              progress: 0,
              downBytesPerSecond: down,
              upBytesPerSecond: up,
              peers: livePeers,
              etaSecs: null,
            )
          : null,
    );

void main() {
  group('generated collection presentation', () {
    test('available content is sharing while an empty summary is not', () {
      expect(_c().isSharingFor(null), isTrue);
      expect(_c(entries: 0).isSharingFor(null), isFalse);
    });

    test('glow is derived from generated state and throughput', () {
      expect(_c(status: 'Preparing').glowFor(null), GlowLevel.none);
      expect(_c().glowFor(null), GlowLevel.calm);
      expect(_c(status: 'Downloading', down: 125000).glowFor(null),
          GlowLevel.active);
      expect(
          _c(status: 'Available', up: 1125000).glowFor(null), GlowLevel.vivid);
    });

    test('live intensity uses both generated transfer directions', () {
      expect(_c().liveIntensity, 0);
      expect(
        _c(down: 200000).liveIntensity,
        lessThan(_c(down: 750000).liveIntensity),
      );
      expect(_c(up: 500000, down: 500000).liveIntensity, 1.0);
    });
  });
}
