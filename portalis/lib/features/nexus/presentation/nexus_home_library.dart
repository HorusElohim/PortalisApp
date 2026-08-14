import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../../collections/presentation/command_bar.dart';
import '../domain/nexus_app_state.dart';

/// The Home collection projection rendered directly from the app-owned Nexus
/// state. It intentionally does not translate Nexus collections into the
/// legacy collection model: Home now has one source of truth.
class NexusHomeLibrary extends StatelessWidget {
  const NexusHomeLibrary({
    super.key,
    required this.wide,
    required this.state,
    required this.error,
    required this.query,
    required this.filter,
    required this.onSearch,
    required this.onFilterChanged,
    required this.onImportTorrent,
    required this.onJoin,
    required this.onOpen,
  });

  final bool wide;
  final NexusAppState? state;
  final String? error;
  final String query;
  final NexusCollectionFilter filter;
  final ValueChanged<String> onSearch;
  final ValueChanged<NexusCollectionFilter> onFilterChanged;
  final Future<void> Function(String source) onImportTorrent;
  final ValueChanged<String> onJoin;
  final ValueChanged<NexusCollection> onOpen;

  List<NexusCollection> get _collections => state?.collections ?? const [];

  List<NexusCollection> get _shown => _collections
      .where((collection) => _matchesQuery(collection))
      .where(filter.includes)
      .toList(growable: false);

  bool _matchesQuery(NexusCollection collection) =>
      query.isEmpty ||
      collection.name.toLowerCase().contains(query.toLowerCase());

  @override
  Widget build(BuildContext context) => wide ? _wide() : _compact();

  Widget _toolbar({required bool showFilters}) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PortalisCommandBar(
            onSearch: onSearch,
            onInvite: onJoin,
            onImportTorrent: onImportTorrent,
          ),
          if (showFilters) ...[
            const SizedBox(height: 14),
            FilterChips(
              labels: const ['All', 'Sharing', 'Receiving'],
              selected: NexusCollectionFilter.values.indexOf(filter),
              onSelected: (index) =>
                  onFilterChanged(NexusCollectionFilter.values[index]),
            ),
          ],
        ],
      );

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
              child: _toolbar(showFilters: true),
            ),
            Expanded(child: _body(padding: kScreenGutter)),
          ],
        ),
      );

  Widget _compact() => PageBody(
        child: CustomScrollView(
          slivers: [
            SliverToBoxAdapter(child: _compactHeader()),
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(22, 20, 22, 18),
                child: _toolbar(showFilters: _collections.length > 1),
              ),
            ),
            SliverFillRemaining(
              hasScrollBody: _shown.isNotEmpty,
              child: _body(padding: 22),
            ),
          ],
        ),
      );

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

  Widget _body({required double padding}) {
    if (state == null && error == null) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_shown.isNotEmpty) {
      return ListView.separated(
        padding: EdgeInsets.fromLTRB(padding, 0, padding, 28),
        itemCount: _shown.length,
        separatorBuilder: (_, __) => const SizedBox(height: 12),
        itemBuilder: (_, index) => NexusCollectionCard(
          collection: _shown[index],
          onTap: () => onOpen(_shown[index]),
        ),
      );
    }
    final message = error ??
        (query.isNotEmpty
            ? 'Nothing matches "$query".'
            : filter == NexusCollectionFilter.all
                ? 'Import a .torrent file or paste a magnet URI to begin.'
                : 'Nothing matches this view yet.');
    return Padding(
      padding: EdgeInsets.symmetric(horizontal: padding),
      child: Center(
        child: Text(
          message,
          textAlign: TextAlign.center,
          style: AppText.body(color: AppColors.textDim),
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

/// Filters are derived from Nexus roles and collection state, never a legacy
/// model's transfer flags.
enum NexusCollectionFilter { all, sharing, receiving }

extension on NexusCollectionFilter {
  bool includes(NexusCollection collection) => switch (this) {
        NexusCollectionFilter.all => true,
        NexusCollectionFilter.sharing =>
          collection.role == 'Owner' && collection.status == 'Available',
        NexusCollectionFilter.receiving => collection.status == 'Preparing' ||
            collection.status == 'Downloading',
      };
}

class NexusCollectionCard extends StatelessWidget {
  const NexusCollectionCard({
    super.key,
    required this.collection,
    required this.onTap,
  });

  final NexusCollection collection;
  final VoidCallback onTap;

  bool get _isTorrent =>
      collection.status == 'Preparing' ||
      collection.status == 'Downloading' ||
      collection.transfer != null;

  @override
  Widget build(BuildContext context) {
    final transfer = collection.transfer;
    final accent = _isTorrent ? AppColors.ember : AppColors.signal;
    return SurfaceCard(
      onTap: onTap,
      glow: transfer == null ? GlowLevel.none : GlowLevel.active,
      glowColor: accent,
      glowIntensity: transfer == null ? 0 : transfer.progress,
      child: Row(
        children: [
          _CollectionIcon(torrent: _isTorrent, accent: accent),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  collection.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: displayText(size: 16),
                ),
                const SizedBox(height: 4),
                Text(
                  '${plural(collection.entries, 'file')} · '
                  '${formatBytes(collection.totalBytes.toInt())}',
                  style: monoLabel(size: 10.5, color: AppColors.textDim),
                ),
                if (transfer != null) ...[
                  const SizedBox(height: 10),
                  ClipRRect(
                    borderRadius: BorderRadius.circular(AppRadius.pill),
                    child: LinearProgressIndicator(
                      value: transfer.progress.clamp(0, 1),
                      minHeight: 5,
                      backgroundColor: AppColors.borderStrong,
                      valueColor: AlwaysStoppedAnimation(accent),
                    ),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(width: 12),
          StatusBadge(
            label: _statusLabel(collection.status),
            color: transfer == null ? null : accent,
          ),
        ],
      ),
    );
  }
}

class _CollectionIcon extends StatelessWidget {
  const _CollectionIcon({required this.torrent, required this.accent});

  final bool torrent;
  final Color accent;

  @override
  Widget build(BuildContext context) => Container(
        width: 50,
        height: 50,
        decoration: BoxDecoration(
          color: accent.withValues(alpha: 0.12),
          borderRadius: BorderRadius.circular(AppRadius.control),
          border: Border.all(color: accent.withValues(alpha: 0.28)),
        ),
        child: Icon(
          torrent ? Icons.download_outlined : Icons.folder_shared_outlined,
          color: accent,
        ),
      );
}

String _statusLabel(String status) => switch (status) {
      'Preparing' => 'PREPARING',
      'Downloading' => 'DOWNLOADING',
      'Available' => 'AVAILABLE',
      'Updating' => 'UPDATING',
      'WaitingForOwner' => 'WAITING',
      'AccessRemoved' => 'REMOVED',
      'NeedsNewerVersion' => 'UPDATE APP',
      'ConflictingHistory' => 'CONFLICT',
      _ => status.toUpperCase(),
    };
