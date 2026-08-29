import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/design/design.dart';
import 'package:portalis/features/settings/presentation/diagnostics_screen.dart';
import 'package:portalis/features/settings/presentation/formats_screen.dart';
import 'package:portalis/features/settings/presentation/storage_screen.dart';

/// Every screen reached by drilling into Settings must read at the same
/// width as Settings itself on a spacious window — see
/// `PageBody.settingsWideMaxWidth`. Diagnostics, Storage and File formats
/// used to silently fall back to the app's narrower default reading
/// measure instead, so a screen opened from Settings looked visibly
/// cramped next to the screen it was opened from.
void main() {
  const spacious = Size(1600, 1000);

  double pageBodyMaxWidth(WidgetTester tester) =>
      tester.widget<PageBody>(find.byType(PageBody)).wideMaxWidth;

  group('settings family width consistency', () {
    testWidgets('Diagnostics matches the Settings-family width', (tester) async {
      await tester.binding.setSurfaceSize(spacious);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        const MaterialApp(home: DiagnosticsScreen()),
      );
      await tester.pump();

      expect(pageBodyMaxWidth(tester), PageBody.settingsWideMaxWidth);
    });

    testWidgets('Storage matches the Settings-family width', (tester) async {
      await tester.binding.setSurfaceSize(spacious);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        const MaterialApp(home: StorageScreen()),
      );
      await tester.pump();

      expect(pageBodyMaxWidth(tester), PageBody.settingsWideMaxWidth);
    });

    testWidgets('File formats matches the Settings-family width', (tester) async {
      await tester.binding.setSurfaceSize(spacious);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        const MaterialApp(home: FormatsScreen()),
      );
      await tester.pump();

      expect(pageBodyMaxWidth(tester), PageBody.settingsWideMaxWidth);
    });
  });
}
