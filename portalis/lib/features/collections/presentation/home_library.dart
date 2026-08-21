import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import 'commands.dart';
import 'views.dart';
import 'list.dart';
import 'share_action.dart';
import '../../../nexus/domain/app_state.dart';

/// The Home collection projection, drawn by the same row and list widgets the
/// legacy collections library always used.
///
/// A wide window grows a row into its own detail in place — [CollectionsList]
/// and [CollectionRow]'s own accordion, tracked here only by which id is
/// open. A narrow one shows a plain [CollectionRow] per collection with its
/// command bar always visible, exactly as it always did; opening one is the
/// caller's business (see [Home.onOpen]), not this widget's.
///
/// Rows consume Nexus summaries directly and make no data translation.
class HomeLibrary extends StatelessWidget {
  const HomeLibrary({
    super.key,
    required this.wide,
    required this.state,
    required this.error,
    required this.onCreateCollection,
    required this.onOpen,
    required this.onCommand,
  });

  final bool wide;
  final AppSnapshot? state;
  final String? error;
  final VoidCallback onCreateCollection;
  final ValueChanged<AppCollection> onOpen;
  final ValueChanged<(AppCollection, CollectionCommand)> onCommand;

  /// The one collection currently grown into its own detail. Only meaningful
  /// wide — a narrow list has nowhere to grow a row into, so nothing here is
  /// ever "open" on it.

  List<AppCollection> get _collections => state?.collections ?? const [];

  List<AppCollection> get _shown => _collections
      .where((collection) => _matchesQuery(collection))
      .toList(growable: false);

  bool _matchesQuery(AppCollection collection) => true;

  @override
  Widget build(BuildContext context) => wide ? _wide() : _compact();

  /// Sharing is the one thing a person can always do, so it is never in a
  /// list that might be empty. With collections it sits above them; without
  /// any it is the middle of the screen, because there is nothing else there
  /// to be the subject.
  Widget _shareAction() => ShareCollectionAction(onTap: onCreateCollection);

  Widget _wide() => AppScreen(
        // No title: this pane is the whole window, reached by the button that
        // is always lit. Naming it "Home" spent a headline saying what a
        // person was already looking at.
        subtitle: Text(_summary),
        embedded: true,
        width: ScreenWidth.full,
        body: Column(
          children: [
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

    return CollectionsList(
      collections: _shown,
      onOpen: onOpen,
      onCommand: onCommand,
      // Every row expands now. A torrent waiting to be chosen from used to be
      // the exception, because choosing happened on a screen of its own; it
      // happens on the collection itself, so there is nothing left that a row
      // cannot grow into.
    );
  }

  Widget _compact() => PageBody(
        child: CustomScrollView(
          slivers: [
            SliverToBoxAdapter(child: _compactHeader()),
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
            collection: collection,
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
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 22),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _shareAction(),
            const SizedBox(height: 18),
            Text(
              'Share photos or files, or fetch a torrent. Nothing leaves this '
              'device until you say so.',
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
        .whereType<AppTransfer>()
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
        '↓ ${formatRate(down)} · ↑ ${formatRate(up)}';
  }
}
