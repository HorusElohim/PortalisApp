import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/design/polling.dart';

class _Probe extends StatefulWidget {
  const _Probe({required this.onPoll});

  final VoidCallback onPoll;

  @override
  State<_Probe> createState() => _ProbeState();
}

class _ProbeState extends State<_Probe> with PollingState<_Probe> {
  @override
  Duration get pollInterval => const Duration(seconds: 2);

  @override
  void initState() {
    super.initState();
    startPolling();
  }

  @override
  void onPoll() => widget.onPoll();

  @override
  Widget build(BuildContext context) => const SizedBox.shrink();
}

void main() {
  group('PollingState', () {
    testWidgets('fires immediately, then on every tick', (tester) async {
      var calls = 0;
      await tester.pumpWidget(_Probe(onPoll: () => calls++));

      expect(calls, 1, reason: 'starts polling fires once immediately');

      await tester.pump(const Duration(seconds: 2));
      expect(calls, 2);

      await tester.pump(const Duration(seconds: 2));
      expect(calls, 3);
    });

    testWidgets('stops polling once disposed', (tester) async {
      var calls = 0;
      await tester.pumpWidget(_Probe(onPoll: () => calls++));
      expect(calls, 1);

      // Unmount by pumping something else in its place.
      await tester.pumpWidget(const SizedBox.shrink());

      // Advancing time after disposal must not touch a widget that is gone
      // — a Timer that outlives its State is a leak and, if it ever calls
      // setState, a crash.
      await tester.pump(const Duration(seconds: 10));
      expect(calls, 1);
      expect(tester.takeException(), isNull);
    });
  });
}
