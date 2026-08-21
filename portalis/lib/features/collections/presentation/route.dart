import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/application/app_controller.dart';
import '../../../nexus/domain/app_state.dart';
import 'detail.dart';

Widget routeFor(AppCollection collection, AppController controller) =>
    CollectionRoute(collection: collection.id, controller: controller);

/// Owns exactly the selected collection's tiered detail and history streams.
class CollectionRoute extends StatefulWidget {
  const CollectionRoute({
    super.key,
    required this.collection,
    required this.controller,
  });

  final int collection;
  final AppController controller;

  @override
  State<CollectionRoute> createState() => _CollectionRouteState();
}

class _CollectionRouteState extends State<CollectionRoute> {
  AppDetail? _detail;
  final List<Reading> _readings = [];
  StreamSubscription<AppDetail?>? _detailSubscription;
  StreamSubscription<Uint8List>? _historySubscription;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_changed);
    _detailSubscription =
        widget.controller.watchDetail(widget.collection).listen((detail) {
      if (!mounted) return;
      setState(() => _detail = detail);
    });
    _historySubscription =
        widget.controller.watchHistory(widget.collection).listen((packed) {
      if (!mounted) return;
      setState(() {
        _readings.addAll(decodeReadings(packed));
        if (_readings.length > 1800) {
          _readings.removeRange(0, _readings.length - 1800);
        }
      });
    });
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    widget.controller.removeListener(_changed);
    unawaited(_detailSubscription?.cancel());
    unawaited(_historySubscription?.cancel());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = widget.controller.state;
    final collection = state?.collections
        .where((item) => item.id == widget.collection)
        .firstOrNull;
    if (collection == null) return const _CollectionUnavailable();
    return CollectionScreen(
      collection: collection,
      detail: _detail,
      readings: _readings,
      contacts: state?.contacts ?? const [],
      controller: widget.controller,
    );
  }
}

class _CollectionUnavailable extends StatelessWidget {
  const _CollectionUnavailable();

  @override
  Widget build(BuildContext context) => Scaffold(
        backgroundColor: AppColors.surfaceDeep,
        body: SafeArea(
          child: PageBody(
            child: Padding(
              padding: const EdgeInsets.all(kScreenGutter),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  NavBackButton(onTap: () => Navigator.of(context).pop()),
                  const Padding(
                    padding: EdgeInsets.only(top: 40),
                    child: Center(
                      child: Text('This collection is no longer available.'),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
}
