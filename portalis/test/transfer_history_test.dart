import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
  test('decodes append-only generated history rows once at the gateway edge',
      () {
    final bytes = Uint8List(18);
    ByteData.sublistView(bytes)
      ..setUint64(0, 1000000000)
      ..setUint32(8, 525000)
      ..setUint32(12, 37500)
      ..setUint16(16, 800);

    final readings = decodeReadings(bytes);

    expect(readings, hasLength(1));
    expect(readings.single.downBytesPerSecond, 525000);
    expect(readings.single.upBytesPerSecond, 37500);
    expect(readings.single.progress, .8);
  });
}
