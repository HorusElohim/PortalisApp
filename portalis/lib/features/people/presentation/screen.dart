import 'dart:async';

import 'package:flutter/material.dart';

import '../../../app/app_controllers.dart';
import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';
import '../../collections/domain/peer_observation.dart';
import '../../collections/presentation/peer_color.dart';

/// The people and machines this device is connected to.
///
/// Two tiers, deliberately kept apart. A *contact* is somebody this device
/// holds signed statements about, whose fingerprint a person can compare out
/// of band. A *connection* is a swarm peer: an address moving bytes right now,
/// with no signed identity behind it and a self-reported client name at best.
///
/// Merging them would put strangers beside people, wearing the same card. So
/// they are separate sections with different shapes, and the connection tier
/// leads with what is actually knowable about it — how much it has sent and
/// received, and how fast.
class PeopleScreen extends StatefulWidget {
  const PeopleScreen({super.key, this.embedded = false});

  final bool embedded;

  @override
  State<PeopleScreen> createState() => _PeopleScreenState();
}

class _PeopleScreenState extends State<PeopleScreen> {
  /// Peers are polled rather than streamed: they change every engine tick, and
  /// a stream would rebuild this screen far faster than a person can read it.
  static const _refresh = Duration(seconds: 2);

  Timer? _timer;
  List<AppCollectionPeer> _peers = const [];

  @override
  void initState() {
    super.initState();
    unawaited(_load());
    // Also whenever the engine reports anything: a collection appearing or a
    // transfer starting changes who is connected, and waiting up to a full
    // refresh interval to notice would make this screen lag the rest of the
    // app for no reason.
    AppControllers.engine.addListener(_load);
    _timer = Timer.periodic(_refresh, (_) => unawaited(_load()));
  }

  @override
  void dispose() {
    AppControllers.engine.removeListener(_load);
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    final peers = await AppControllers.engine.peers();
    if (!mounted) return;
    setState(() => _peers = peers);
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: AppControllers.engine,
        builder: (context, _) => _build(context),
      );

  Widget _build(BuildContext context) {
    final state = AppControllers.engine.state;
    final contacts = state?.contacts ?? const <AppContact>[];
    final collections = state?.collections ?? const <AppCollection>[];

    // Where each contact actually appears, so a card can say more than that
    // they exist.
    final entries = [
      for (final contact in contacts)
        (
          contact: contact,
          collections: [
            for (final collection in collections)
              if (collection.members.contains(contact.id)) collection.name,
          ],
        ),
    ];

    final connections = _connections(collections);
    final isEmpty = entries.isEmpty && connections.isEmpty;

    return AppScreen(
      title: 'People',
      subtitle: isEmpty ? null : Text(_summary(entries.length, connections)),
      embedded: widget.embedded,
      width: ScreenWidth.full,
      body: isEmpty
          ? Center(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: kScreenGutter),
                child: Text(
                  'Nobody yet. People appear here once you add a contact, and '
                  'connections appear while a collection is transferring.',
                  textAlign: TextAlign.center,
                  style: AppText.body(color: AppColors.textDim),
                ),
              ),
            )
          : WindowBuilder(
              builder: (context, window) => ListView(
                padding: const EdgeInsets.fromLTRB(
                  kScreenGutter,
                  0,
                  kScreenGutter,
                  28,
                ),
                children: [
                  if (entries.isNotEmpty) ...[
                    const SectionLabel('CONTACTS'),
                    const SizedBox(height: 8),
                    GridView.builder(
                      shrinkWrap: true,
                      physics: const NeverScrollableScrollPhysics(),
                      gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                        crossAxisCount: window.columns(340),
                        mainAxisSpacing: 10,
                        crossAxisSpacing: 10,
                        mainAxisExtent: 190,
                      ),
                      itemCount: entries.length,
                      itemBuilder: (context, index) => ContactCard(
                        contact: entries[index].contact,
                        collections: entries[index].collections,
                      ),
                    ),
                    const SizedBox(height: 22),
                  ],
                  if (connections.isNotEmpty) ...[
                    SectionLabel('CONNECTIONS - ${connections.length}'),
                    const SizedBox(height: 6),
                    Text(
                      'Swarm peers moving bytes right now. These are network '
                      'addresses, not identified people.',
                      style: AppText.secondary(color: AppColors.textDim),
                    ),
                    const SizedBox(height: 10),
                    for (final connection in connections)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 8),
                        child: ConnectionCard(peer: connection),
                      ),
                  ],
                ],
              ),
            ),
    );
  }

  /// Every live peer across every collection, busiest first.
  ///
  /// The same address can appear on two collections and is listed once per
  /// collection: it is one connection per torrent, and pooling them would
  /// invent a single relationship the protocol does not have.
  List<PeerObservation> _connections(List<AppCollection> collections) {
    final names = {
      for (final collection in collections) collection.id: collection.name,
    };
    final now = DateTime.now();
    final observations = [
      for (final entry in _peers)
        PeerObservation(
          collectionId: '${entry.collection}',
          collectionName: names[entry.collection] ?? 'Unknown collection',
          address: entry.peer.address,
          lastSeen: now,
          client: entry.peer.client,
          downBytes: entry.peer.downBytes.toInt(),
          upBytes: entry.peer.upBytes.toInt(),
          downBytesPerSecond: entry.peer.downBytesPerSecond,
          upBytesPerSecond: entry.peer.upBytesPerSecond,
        ),
    ];
    observations.sort((a, b) {
      final moving = (b.downBytesPerSecond + b.upBytesPerSecond)
          .compareTo(a.downBytesPerSecond + a.upBytesPerSecond);
      if (moving != 0) return moving;
      return (b.downBytes + b.upBytes).compareTo(a.downBytes + a.upBytes);
    });
    return observations;
  }

  String _summary(int contacts, List<PeerObservation> connections) {
    final parts = [
      if (contacts > 0) plural(contacts, 'contact'),
      if (connections.isNotEmpty) plural(connections.length, 'connection'),
    ];
    return parts.join(' · ');
  }
}

/// One person this device holds signed statements about.
///
/// Shows no transfer figures. Neither engine attributes bytes to a signed
/// identity — a collection knows what it moved, not which of its members
/// consumed it — and the card this replaces showed a pooled collection total
/// wearing a person's name. Bytes belong to the connection tier below, where
/// they are measured rather than attributed.
class ContactCard extends StatelessWidget {
  const ContactCard({
    super.key,
    required this.contact,
    required this.collections,
  });

  final AppContact contact;
  final List<String> collections;

  bool get _reachable => contact.reachable != null;

  @override
  Widget build(BuildContext context) => SurfaceCard(
        padding: const EdgeInsets.all(16),
        glow: _reachable ? GlowLevel.calm : GlowLevel.none,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Avatar(initials: _initials, size: 44),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        contact.displayName,
                        overflow: TextOverflow.ellipsis,
                        style: AppText.cardTitle(),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        contact.handle ?? _friendship,
                        overflow: TextOverflow.ellipsis,
                        style: monoLabel(
                          size: 9.5,
                          color: AppColors.textDim,
                          letterSpacing: 0.4,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                // Verification is the one thing about a contact a person has
                // to act on (D4), so it is the badge rather than a detail.
                StatusBadge(
                  label: contact.verified ? 'VERIFIED' : 'UNVERIFIED',
                  color: contact.verified ? AppColors.signal : null,
                ),
              ],
            ),
            const SizedBox(height: 14),
            Text(
              collections.isEmpty
                  ? 'No shared collections yet'
                  : collections.join(' · '),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: AppText.secondary(color: AppColors.textDim),
            ),
            const Spacer(),
            // The fingerprint is what a person compares out of band to make
            // "verified" mean anything, so it is shown rather than hidden
            // behind a tap.
            Text(
              contact.fingerprint,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: monoLabel(size: 9, color: AppColors.textFaint),
            ),
          ],
        ),
      );

  String get _initials {
    final name = contact.displayName.trim();
    return name.isEmpty ? '?' : name[0].toUpperCase();
  }

  String get _friendship => switch (contact.friendship) {
        'Requested' => 'REQUEST SENT',
        'Pending' => 'WANTS TO CONNECT',
        'Blocked' => 'BLOCKED',
        _ => _reachable ? 'CONNECTED' : 'OFFLINE',
      };
}

/// One live swarm connection, with what it has actually exchanged.
///
/// Deliberately not shaped like a contact card: no avatar, no verification
/// badge, no name in the title position. The address leads because it is the
/// only thing here this device can vouch for. The client name is shown as a
/// claim beside it, and the byte figures — which *are* this device's own
/// measurements — get the emphasis a person actually wants.
class ConnectionCard extends StatelessWidget {
  const ConnectionCard({super.key, required this.peer});

  final PeerObservation peer;

  @override
  Widget build(BuildContext context) {
    final color =
        peer.isMoving ? AppColors.ember : rememberedPeerColor(peer.address);
    return SurfaceCard(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      glow: peer.isMoving ? GlowLevel.active : GlowLevel.none,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(color: color, shape: BoxShape.circle),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  peer.address,
                  overflow: TextOverflow.ellipsis,
                  style: monoLabel(size: 11.5, letterSpacing: 0),
                ),
                const SizedBox(height: 3),
                Text(
                  _subtitle,
                  overflow: TextOverflow.ellipsis,
                  style: AppText.secondary(color: AppColors.textDim),
                ),
              ],
            ),
          ),
          const SizedBox(width: 10),
          _Exchanged(peer: peer),
        ],
      ),
    );
  }

  /// What the peer says it is, and which collection this connection belongs
  /// to. The client name is prefixed as reported so the card never presents a
  /// self-chosen string as though this device verified it.
  String get _subtitle {
    final client = peer.client;
    return client == null
        ? peer.collectionName
        : '${peer.collectionName} · reports $client';
  }
}

/// The transfer figures for one connection.
///
/// Rates when something is moving, totals when it is not. A connected peer
/// that has gone quiet says so rather than showing a stale rate, and one that
/// has exchanged nothing at all says that too — it is a real and common state.
class _Exchanged extends StatelessWidget {
  const _Exchanged({required this.peer});

  final PeerObservation peer;

  @override
  Widget build(BuildContext context) {
    if (peer.isMoving) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (peer.downBytesPerSecond > 0)
            Text(
              '↓ ${formatRate(peer.downBytesPerSecond)}',
              style: monoLabel(
                size: 11,
                color: AppColors.signal,
                letterSpacing: 0,
              ),
            ),
          if (peer.upBytesPerSecond > 0)
            Text(
              '↑ ${formatRate(peer.upBytesPerSecond)}',
              style: monoLabel(
                size: 11,
                color: AppColors.ember,
                letterSpacing: 0,
              ),
            ),
          const SizedBox(height: 2),
          Text(
            _totals,
            style: monoLabel(size: 9, color: AppColors.textFaint),
          ),
        ],
      );
    }
    return Text(
      peer.hasExchanged ? _totals : 'connected · idle',
      textAlign: TextAlign.end,
      style: monoLabel(size: 10, color: AppColors.textDim, letterSpacing: 0),
    );
  }

  String get _totals =>
      '↓ ${formatBytes(peer.downBytes)} · ↑ ${formatBytes(peer.upBytes)}';
}
