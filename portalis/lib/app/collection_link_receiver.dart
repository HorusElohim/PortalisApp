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
  String? _handled;

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
    if (uri == null || _handled == uri.toString()) return;
    try {
      final collection = await importCollectionLink(
        uri,
        send: AppControllers.engine.send,
      );
      if (collection == null) return;
      _handled = uri.toString();
      unawaited(_startDownload(collection));
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
    }
  }

  Future<void> _startDownload(int collection) async {
    try {
      await startCollectionLinkDownload(
        collection,
        send: AppControllers.engine.send,
        watchDetail: AppControllers.engine.watchDetail,
      );
    } catch (error) {
      // The import route stays visible if metadata or the torrent download
      // cannot start; this must not fall through to the local Share workflow.
      debugPrint('Portalis collection download did not start: $error');
    }
  }
}
