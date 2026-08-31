import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/design/design.dart';

void main() {
  testWidgets('waiting indicator pairs a spinner with an honest wait message',
      (tester) async {
    await tester.pumpWidget(const MaterialApp(
      home: Scaffold(body: WaitingIndicator(message: 'Connecting to peers…')),
    ));

    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.text('Connecting to peers…'), findsOneWidget);
  });
}
