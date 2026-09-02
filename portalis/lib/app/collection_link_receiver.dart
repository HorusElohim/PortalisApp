import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/material.dart';

import '../features/collections/presentation/route.dart';
import '../shell/navigation.dart';
import 'app_controllers.dart';
import 'collection_link.dart';

final _collectionLinkReceiver = CollectionLinkReceiver();

/// Starts the process-lifetime receiver for `portalis://import` links.
void startCollectionLinkReceiver() =>
    unawaited(_collectionLinkReceiver.start());

/// Adapts OS URL delivery to the existing validated collection-import command.
class CollectionLinkReceiver {
  CollectionLinkReceiver({AppLinks? links}) : _links = links ?? AppLinks();

  final AppLinks _links;
  Future<void>? _starting;

  /// A link whose import succeeded and already navigated — a later delivery
  /// of the identical URI must not navigate again.
  String? _handled;

  /// Links currently mid-import. Rust owns import identity and returns the
  /// same collection for concurrent equivalent sources (ADR-0015), but
  /// Flutter is not the deduplication authority for that — it is, however,
  /// the one authority for how many times *navigation* fires. Without this,
  /// two deliveries of the same URI landing before either's `await` resolves
  /// (the OS's initial link and its own stream both firing for one URI is
  /// the common real case) would both pass the `_handled` check, both call
  /// import, and — since Rust now correctly answers both with the same
  /// collection — both would still push a second, redundant route.
  ///
  /// Cleared unconditionally when an attempt finishes, success or failure,
  /// so a failed import never permanently blocks a genuine future retry of
  /// the same link — only concurrent duplicates of one attempt are merged.
  final Set<String> _inFlight = {};

  Future<void> start() => _starting ??= _start();

  Future<void> _start() async {
    _links.uriLinkStream.listen(
      (uri) => unawaited(_receive(uri)),
      onError: (Object error, StackTrace stackTrace) {
        debugPrint('Portalis collection-link stream failed: $error');
      },
    );
    try {
      await _receive(await _links.getInitialLink());
    } catch (error) {
      // A link receiver must not stop an otherwise healthy app from opening.
      debugPrint('Portalis initial collection link failed: $error');
    }
  }

  Future<void> _receive(Uri? uri) async {
    if (uri == null) return;
    final key = uri.toString();
    if (_handled == key || !_inFlight.add(key)) return;
    try {
      final collection = await importCollectionLink(
        uri,
        send: AppControllers.engine.send,
      );
      if (collection == null) return;
      _handled = key;
      AppNavigation.goHome();
      WidgetsBinding.instance.addPostFrameCallback((_) {
        final navigator = AppNavigation.navigatorKey.currentState;
        if (navigator == null) return;
        navigator.push(MaterialPageRoute(
          builder: (_) => CollectionRoute(
            collection: collection,
            controller: AppControllers.engine,
          ),
        ));
      });
    } catch (error) {
      // The link is external input; report its failure without taking down the
      // app or navigating somewhere that suggests an import succeeded.
      debugPrint('Portalis collection link was not imported: $error');
    } finally {
      _inFlight.remove(key);
    }
  }
}
