import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../../collections/domain/collection.dart';
import '../../collections/presentation/collection_commands.dart';
import '../../collections/presentation/collection_detail.dart';
import '../../collections/presentation/collection_source.dart';
import '../../collections/presentation/collection_views.dart';
import '../../collections/presentation/collections_list.dart';
import '../../collections/presentation/command_bar.dart';
import '../../collections/presentation/share_collection_action.dart';
import '../data/nexus_collection_view.dart';
import 'nexus_collection_detail.dart' show nexusCollectionNeedsSelection;
import '../domain/nexus_app_state.dart';

/// The Home collection projection, drawn by the same row and list widgets the
/// legacy collections library always used.
///
/// A wide window grows a row into its own detail in place — [CollectionsList]
/// and [CollectionRow]'s own accordion, tracked here only by which id is
/// open. A narrow one shows a plain [CollectionRow] per collection with its
/// command bar always visible, exactly as it always did; opening one is the
/// caller's business (see [Home.onOpen]), not this widget's.
///
/// [nexusCollectionView] is what makes reusing those widgets possible: this
/// file translates Nexus's vocabulary into theirs and otherwise makes no
/// rendering decision of its own.
class NexusHomeLibrary extends StatelessWidget {
  const NexusHomeLibrary({
    super.key,
    required this.wide,
    required this.state,
    required this.error,
    required this.query,
    required this.onSearch,
    required this.onImportTorrent,
    required this.onCreateCollection,
    required this.onJoin,
    required this.onOpen,
    required this.onCommand,
    this.openId,
    this.openSource,
  });

  final bool wide;
  final NexusAppState? state;
  final String? error;
  final String query;
  final ValueChanged<String> onSearch;
  final Future<void> Function(String source) onImportTorrent;
  final VoidCallback onCreateCollection;
  final ValueChanged<String> onJoin;
  final ValueChanged<NexusCollection> onOpen;
  final ValueChanged<(NexusCollection, CollectionCommand)> onCommand;

  /// The one collection currently grown into its own detail. Only meaningful
  /// wide — a narrow list has nowhere to grow a row into, so nothing here is
  /// ever "open" on it.
  final int? openId;

  /// Feeds the currently-open row's [CollectionDetail]. Present exactly when
  /// [openId] is, and owned by whoever set [openId] — this widget only reads
  /// it, matching how [CollectionSource] itself is owned.
  final CollectionSource? openSource;

  List<NexusCollection> get _collections => state?.collections ?? const [];

  List<NexusCollection> get _shown => _collections
      .where((collection) => _matchesQuery(collection))
      .toList(growable: false);

  bool _matchesQuery(NexusCollection collection) =>
      query.isEmpty ||
      collection.name.toLowerCase().contains(query.toLowerCase());

  /// The row summary for one collection — cheap, and never a subscription:
  /// see [nexusCollectionView]'s own doc for what a missing detail costs it.
  Collection _rowView(NexusCollection collection) => nexusCollectionView(
        collection: collection,
        detail: null,
        contacts: state?.contacts ?? const [],
      );

  @override
  Widget build(BuildContext context) => wide ? _wide() : _compact();

  Widget _toolbar() => PortalisCommandBar(
        onSearch: onSearch,
        onInvite: onJoin,
        onImportTorrent: onImportTorrent,
      );

  /// Sharing is the one thing a person can always do, so it is never in a
  /// list that might be empty. With collections it sits above them; without
  /// any it is the middle of the screen, because there is nothing else there
  /// to be the subject.
  Widget _shareAction() => ShareCollectionAction(onTap: onCreateCollection);

  Widget _wide() => AppScreen(
        title: 'Home',
        subtitle: Text(_summary),
        embedded: true,
        width: ScreenWidth.full,
        body: Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                kScreenGutter,
                0,
                kScreenGutter,
                16,
              ),
              child: _toolbar(),
            ),
            if (_shown.isNotEmpty)
              Padding(
                padding: const EdgeInsets.fromLTRB(
                  kScreenGutter,
                  0,
                  kScreenGutter,
                  14,
                ),
                child: _shareAction(),
              ),
            Expanded(child: _wideBody()),
          ],
        ),
      );

  Widget _wideBody() {
    if (state == null && error == null) {
      return const Center(child: CircularProgressIndicator());
    }
    if (error != null) return CollectionsErrorState(message: error!);
    if (_shown.isEmpty) return _emptyState();

    // Rows are built from the summary view, and matched back to their
    // originating [NexusCollection] by id — `CollectionsList`/`CollectionRow`
    // speak only the legacy `Collection`, so this map is what lets their
    // callbacks hand a real Nexus collection back to this widget's own.
    final byRowId = {
      for (final collection in _shown) '${collection.id}': collection,
    };
    return CollectionsList(
      collections: [for (final collection in _shown) _rowView(collection)],
      openId: openId == null ? null : '$openId',
      onOpen: (row) => onOpen(byRowId[row.id]!),
      onCommand: (action) =>
          onCommand((byRowId[action.$1.id]!, action.$2)),
      // A torrent still waiting for file selection has nowhere to grow into —
      // only a screen to go to (see `Home._openCollection`) — so it keeps
      // `CollectionRow`'s plain-tap behaviour rather than the accordion's.
      // Without this, the row's own optimistic "I might be opening" state
      // still fires on the first tap regardless of what `onOpen` decides to
      // do with it, and tries to build a detail for a row that will never
      // actually become the open one.
      canExpand: (row) => !nexusCollectionNeedsSelection(byRowId[row.id]!),
      detailFor: (row, level, inlineHeader, inlineStatus) => CollectionDetail(
        key: ValueKey(row.id),
        collection: row,
        // `detailFor` is only reachable for an expandable row that `openId`
        // names, and `openSource` is its owner's promise that a source
        // exists exactly then — see this widget's own doc. The fallback is
        // defensive rather than load-bearing: it never actually happens.
        source: openSource ?? const LegacyCollectionSource(),
        showCommands: true,
        level: level,
        showTitle: false,
        inlineHeader: inlineHeader,
        inlineStatus: inlineStatus,
      ),
    );
  }

  Widget _compact() => PageBody(
        child: CustomScrollView(
          slivers: [
            SliverToBoxAdapter(child: _compactHeader()),
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(22, 20, 22, 18),
                child: _toolbar(),
              ),
            ),
            if (_shown.isNotEmpty)
              SliverToBoxAdapter(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(22, 0, 22, 14),
                  child: _shareAction(),
                ),
              ),
            _compactBody(),
          ],
        ),
      );

  Widget _compactBody() {
    if (state == null && error == null) {
      return const SliverFillRemaining(
        hasScrollBody: false,
        child: Center(child: CircularProgressIndicator()),
      );
    }
    if (error != null) {
      return SliverFillRemaining(
        hasScrollBody: false,
        child: CollectionsErrorState(message: error!),
      );
    }
    if (_shown.isEmpty) {
      return SliverFillRemaining(hasScrollBody: false, child: _emptyState());
    }
    return SliverPadding(
      padding: const EdgeInsets.fromLTRB(22, 0, 22, 28),
      sliver: SliverList.separated(
        itemCount: _shown.length,
        separatorBuilder: (_, __) => const SizedBox(height: 14),
        itemBuilder: (_, index) {
          final collection = _shown[index];
          return CollectionRow(
            collection: _rowView(collection),
            onTap: () => onOpen(collection),
            onCommand: (command) => onCommand((collection, command)),
          );
        },
      ),
    );
  }

  Widget _compactHeader() {
    final device = state?.device;
    final name = device?.name ?? 'Portalis';
    final initials = name.isEmpty ? '·' : name[0].toUpperCase();
    return Padding(
      padding: const EdgeInsets.fromLTRB(22, 14, 22, 0),
      child: Row(
        children: [
          Avatar(initials: initials, size: 34, primary: true),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(name, style: displayText(size: 16)),
                Text(
                  _summary,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: monoLabel(size: 10, color: AppColors.textDim),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _emptyState() {
    final searching = query.isNotEmpty;
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 22),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Not while searching: an empty result is a fact about the
            // query, and offering to create something answers a question
            // nobody asked.
            if (!searching) ...[
              _shareAction(),
              const SizedBox(height: 18),
            ],
            Text(
              searching
                  ? 'Nothing matches "$query".'
                  : 'Share files, import a .torrent file, or paste a magnet URI '
                      'to begin.',
              textAlign: TextAlign.center,
              style: AppText.body(color: AppColors.textDim),
            ),
          ],
        ),
      ),
    );
  }

  String get _summary {
    final transfers = _collections
        .map((collection) => collection.transfer)
        .whereType<NexusTransfer>()
        .toList(growable: false);
    if (transfers.isEmpty) return plural(_collections.length, 'collection');
    final down = transfers.fold<int>(
      0,
      (total, transfer) => total + transfer.downBytesPerSecond,
    );
    final up = transfers.fold<int>(
      0,
      (total, transfer) => total + transfer.upBytesPerSecond,
    );
    return '${plural(transfers.length, 'active transfer')} · '
        '↓ ${formatRate(down / 1000000)} · ↑ ${formatRate(up / 1000000)}';
  }
}

