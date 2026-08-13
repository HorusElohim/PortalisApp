import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/nexus/application/nexus_app_controller.dart';
import 'package:portalis/features/nexus/data/nexus_app_repository.dart';
import 'package:portalis/features/nexus/domain/nexus_app_state.dart';

void main() {
  test('owns one state subscription and forwards lifecycle changes', () async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    var notifications = 0;
    controller.addListener(() => notifications++);

    await controller.start();
    repository.states.add(_state('Mina'));
    await Future<void>.delayed(Duration.zero);

    expect(repository.starts, 1);
    expect(controller.state?.device.name, 'Mina');
    expect(notifications, 1);

    await controller.start();
    await controller.setActive(false);
    expect(repository.starts, 1, reason: 'the subscription is app-owned');
    expect(repository.active, [false]);

    await controller.stop();
    expect(repository.stops, 1);
  });
}

NexusAppState _state(String name) => NexusAppState(
      device: NexusDevice(
        name: name,
        handle: null,
        fingerprint: 'fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: const [],
      alerts: const [],
    );

class _Repository implements NexusAppRepository {
  final states = StreamController<NexusAppState>.broadcast();
  final active = <bool>[];
  var starts = 0;
  var stops = 0;

  @override
  Future<NexusAccepted> send(NexusCommand command) async =>
      NexusAccepted(id: BigInt.zero, queued: false);

  @override
  Future<void> setActive(bool value) async => active.add(value);

  @override
  Future<void> start() async => starts++;

  @override
  Future<void> stop() async {
    stops++;
    await states.close();
  }

  @override
  Stream<NexusDetail?> watchDetail(int? collection) => const Stream.empty();

  @override
  Stream<NexusAppState> watchStates() => states.stream;
}
