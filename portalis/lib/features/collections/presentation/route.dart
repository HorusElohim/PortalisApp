import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import 'detail.dart';
import '../../../nexus/application/app_controller.dart';
import '../../../nexus/data/collection_source.dart';
import '../../../nexus/data/collection_view.dart';
import '../../../nexus/domain/app_state.dart';

/// Which screen represents [collection] when it is opened as its own route.
///
/// One answer for every collection. A torrent waiting to be chosen from used
/// to get a screen of its own, which made choosing a gate passed through
/// exactly once: afterwards there was nowhere to change your mind, and no
/// screen could show a half-fetched collection alongside the files it had
/// skipped. Choosing now happens on the collection itself, where it stays
/// available for as long as the collection does.
Widget routeFor(
  AppCollection collection,
  AppController controller,
) =>
    CollectionRoute(collection: collection.id, controller: controller);

/// One Nexus collection, on its own screen.
///
/// Not a second implementation of the collection screen: this *is*
/// [CollectionScreen] — the exact widget the legacy collection screen uses —
/// given a [EngineCollectionSource] instead of the legacy collections
/// controller. Every rendering and command decision lives in
/// [CollectionDetail]; this file only wires where its data and its commands
/// go, and owns the source's subscription for as long as this screen does.
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
  // Constructed once, here, so its subscription survives every rebuild this
  // wrapper goes through — and disposed here too, because whoever constructs
  // a source owns it. Built in `initState` rather than lazily so that
  // disposing never has to first create the thing it is disposing.
  late final EngineCollectionSource _source;

  @override
  void initState() {
    super.initState();
    _source = EngineCollectionSource(
      controller: widget.controller,
      collectionId: widget.collection,
    );
  }

  @override
  void dispose() {
    _source.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.controller,
        builder: (context, _) {
          final current = widget.controller.state?.collections
              .where((item) => item.id == widget.collection)
              .firstOrNull;
          if (current == null) {
            return Scaffold(
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
                            child:
                                Text('This collection is no longer available.'),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            );
          }
          return CollectionScreen(
            // A seed only: `_source.resolve` supplies the live answer on
            // every rebuild. This covers the one frame before that answer
            // exists.
            collection: collectionView(
              collection: current,
              detail: null,
              contacts: widget.controller.state?.contacts ?? const [],
            ),
            source: _source,
          );
        },
      );
}
