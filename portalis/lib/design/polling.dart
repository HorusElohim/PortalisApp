// Cross-feature timer-polling primitive.

import 'dart:async';

import 'package:flutter/widgets.dart';

/// A [State] that refreshes on a fixed timer for as long as it is mounted.
///
/// Storage, Diagnostics, and User each hand-rolled the same three lines —
/// a `Timer? _poll` field, a periodic timer started in `initState`, and a
/// cancel in `dispose` — because each screen's own data (disk usage, a log
/// file, activity totals) can change from something happening elsewhere in
/// the app while the screen sits open with nothing pushing it a fresh
/// value. This is that shape, written once.
///
/// Call [startPolling] from `initState` after `super.initState()`; it fires
/// [onPoll] immediately and then on every tick of [pollInterval]. A state
/// that also needs a `dispose` of its own must call `super.dispose()`
/// rather than skip it, same as any other mixin.
mixin PollingState<T extends StatefulWidget> on State<T> {
  Timer? _pollTimer;

  /// How often [onPoll] fires after the first, immediate call. Every
  /// current poller in the app uses the same 2s cadence; override only for
  /// a genuinely different rate.
  Duration get pollInterval => const Duration(seconds: 2);

  /// Called once immediately, then on every tick, until this state is
  /// disposed. Implementations that `await` should check `mounted` before
  /// calling `setState` — the same rule as any other async callback.
  void onPoll();

  void startPolling() {
    onPoll();
    _pollTimer = Timer.periodic(pollInterval, (_) => onPoll());
  }

  @override
  void dispose() {
    _pollTimer?.cancel();
    super.dispose();
  }
}
