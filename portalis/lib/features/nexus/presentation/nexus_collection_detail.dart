import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../../collections/presentation/collection_detail.dart';
import '../application/nexus_app_controller.dart';
import '../data/nexus_collection_source.dart';
import '../data/nexus_collection_view.dart';
import '../domain/nexus_app_state.dart';
import 'nexus_torrent_preparation.dart';

/// A torrent still waiting for file selection: there is nothing to grow a
/// row into yet, only a choice to make, so it always gets a dedicated screen
/// rather than ever becoming the row a list's `openId` names.
///
/// One decision, shared by every caller that has to make it — the wide list
/// (which needs it to keep such a row from trying to expand inline; see
/// `NexusHomeLibrary`), [Home]'s own push fallback, and the desktop shell's
/// inline-toggle path. A second copy of this branch is exactly the kind of
/// drift that made the pushed screen and the inline one disagree before.
bool nexusCollectionNeedsSelection(NexusCollection collection) =>
    collection.nature == 'Torrent' && collection.status == 'Preparing';

/// Which screen represents [collection] when it is opened as its own route.
Widget nexusCollectionScreen(
  NexusCollection collection,
  NexusAppController controller,
) =>
    nexusCollectionNeedsSelection(collection)
        ? NexusTorrentPreparation(collection: collection.id, controller: controller)
        : NexusCollectionDetail(collection: collection.id, controller: controller);

/// One Nexus collection, on its own screen.
///
/// Not a second implementation of the collection screen: this *is*
/// [CollectionScreen] — the exact widget the legacy collection screen uses —
/// given a [NexusCollectionSource] instead of the legacy collections
/// controller. Every rendering and command decision lives in
/// [CollectionDetail]; this file only wires where its data and its commands
/// go, and owns the source's subscription for as long as this screen does.
class NexusCollectionDetail extends StatefulWidget {
  const NexusCollectionDetail({
    super.key,
    required this.collection,
    required this.controller,
  });

  final int collection;
  final NexusAppController controller;

  @override
  State<NexusCollectionDetail> createState() => _NexusCollectionDetailState();
}

class _NexusCollectionDetailState extends State<NexusCollectionDetail> {
  // Constructed once, here, so its subscription survives every rebuild this
  // wrapper goes through. `CollectionDetail`'s own state is what disposes
  // it — see `NexusCollectionSource`'s doc comment for why this widget must
  // not also dispose it.
  late final NexusCollectionSource _source = NexusCollectionSource(
    controller: widget.controller,
    collectionId: widget.collection,
  );

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
            collection: nexusCollectionView(
              collection: current,
              detail: null,
              contacts: widget.controller.state?.contacts ?? const [],
            ),
            source: _source,
          );
        },
      );
}
