import 'test_support.dart';
import 'package:portalis/features/collections/presentation/peer_color.dart';

/// Sharing means somebody is actually being served, which is why it needs a
/// peer as well as a state — see `Collection.isSharing`.
Collection _c({
  String status = 'Available',
  int down = 0,
  int up = 0,
  int livePeers = 0,
  List<AppEntry>? entries,
}) =>
    buildCollection(
      name: 'Trip',
      status: status,
      downBytesPerSecond: down,
      upBytesPerSecond: up,
      livePeers: livePeers,
      entries: entries ?? [buildEntry()],
    );

void main() {
  group('is it actually being shared', () {
    test('seeding with content is sharing', () {
      expect(_c(status: 'Available').isSharing, isTrue);
    });

    test('an empty collection is not sharing, whatever its state says', () {
      // Nothing to serve means nothing is being served — telling someone
      // their photos are available when no torrent is live is the exact
      // ambiguity this replaces.
      expect(_c(status: 'Available', entries: const []).isSharing, isFalse);
    });

    test('downloading or pending is not sharing', () {
      expect(_c(status: 'Downloading').isSharing, isFalse);
      expect(_c(status: 'Preparing').isSharing, isFalse);
    });
  });

  group('glow is earned, never decorative', () {
    test('an idle, unshared collection has none', () {
      expect(_c(status: 'Preparing').glow, GlowLevel.none);
      expect(_c(status: 'Available', entries: const []).glow, GlowLevel.none);
    });

    test('shared and standing by glows calmly', () {
      expect(_c(status: 'Available').glow, GlowLevel.calm);
    });

    test('transferring glows brighter the faster it goes', () {
      expect(_c(status: 'Downloading', down: 125000).glow, GlowLevel.active);
      expect(_c(status: 'Downloading', down: 1500000).glow, GlowLevel.vivid);
      // Upload counts as much as download — sending is just as alive.
      expect(_c(status: 'Available', up: 1125000).glow, GlowLevel.vivid);
    });
  });

  group('glow tokens', () {
    test('none is genuinely nothing — no shadow, structural border', () {
      final glow = Glow.of(GlowLevel.none);
      expect(glow.isVisible, isFalse);
      expect(glow.shadows, isEmpty);
      expect(glow.border.top.color, AppColors.border);
    });

    test('intensity increases monotonically across the levels', () {
      // The table is the single place the app's energy is tuned, so its
      // ordering has to hold or "brighter" stops meaning "more active".
      final levels = [GlowLevel.calm, GlowLevel.active, GlowLevel.vivid]
          .map(Glow.of)
          .toList();
      for (var i = 1; i < levels.length; i++) {
        expect(levels[i].borderOpacity, greaterThan(levels[i - 1].borderOpacity));
        expect(levels[i].shadowOpacity, greaterThan(levels[i - 1].shadowOpacity));
        expect(levels[i].blur, greaterThan(levels[i - 1].blur));
      }
    });

    test('takes whichever colour it is given, so torrents glow ember', () {
      final glow = Glow.of(GlowLevel.active, color: AppColors.ember);
      // Compare the RGB channels; the alpha is the level's own opacity.
      expect(glow.border.top.color.toARGB32() & 0x00FFFFFF,
          AppColors.ember.toARGB32() & 0x00FFFFFF);
      expect(glow.shadows.single.color.toARGB32() & 0x00FFFFFF,
          AppColors.ember.toARGB32() & 0x00FFFFFF);
    });
  });

  group('the wash follows the glow', () {
    /// Alpha at the bright corner of a level's gradient.
    double top(GlowLevel level, {double intensity = 0}) =>
        (Glow.of(level, intensity: intensity).gradient as LinearGradient)
            .colors
            .first
            .a;

    test('a settled surface has no wash, so it stays flat', () {
      // What lets SurfaceCard fall back to a plain fill without asking.
      expect(Glow.of(GlowLevel.none).gradient, isNull);
    });

    test('brightens across the levels, in step with the halo', () {
      expect(top(GlowLevel.calm), lessThan(top(GlowLevel.active)));
      expect(top(GlowLevel.active), lessThan(top(GlowLevel.vivid)));
    });

    test('brightens with real throughput within a level', () {
      // The dynamic half: same state, more bytes moving, more colour.
      expect(top(GlowLevel.active), lessThan(top(GlowLevel.active, intensity: 1)));
    });

    test('speed alone never lights a surface that is doing nothing', () {
      // Intensity modulates an earned wash; it cannot manufacture one.
      expect(Glow.of(GlowLevel.none, intensity: 1).gradient, isNull);
    });

    test('washes in the colour it glows in', () {
      final wash = Glow.of(GlowLevel.active, color: AppColors.ember).gradient!
          as LinearGradient;
      for (final c in wash.colors) {
        expect(c.toARGB32() & 0x00FFFFFF, AppColors.ember.toARGB32() & 0x00FFFFFF);
      }
    });

    test('stays a wash — never opaque enough to compete with content', () {
      expect(top(GlowLevel.vivid, intensity: 1), lessThan(0.3));
    });
  });

  group('live intensity', () {
    test('a settled collection contributes nothing', () {
      expect(_c(status: 'Available').liveIntensity, 0);
    });

    test('rises with the combined rate', () {
      expect(_c(status: 'Downloading', down: 200000).liveIntensity,
          lessThan(_c(status: 'Downloading', down: 750000).liveIntensity));
      // Both directions count, the same way glow counts them.
      expect(_c(status: 'Available', up: 500000, down: 500000).liveIntensity, 1.0);
    });
  });
}
