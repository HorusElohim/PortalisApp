import 'dart:async';

import 'package:flutter/material.dart';

import '../../../app/app_controllers.dart';
import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';
import '../../collections/domain/peer_observation.dart';
import '../../collections/presentation/peers.dart';

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

class _PeopleScreenState extends State<PeopleScreen> with PollingState {
  List<AppPeoplePeer> _peers = const [];

  @override
  void initState() {
    super.initState();
    // Also whenever the engine reports anything: a collection appearing or a
    // transfer starting changes who is connected, and waiting up to a full
    // refresh interval to notice would make this screen lag the rest of the
    // app for no reason.
    AppControllers.engine.addListener(_load);
    startPolling();
  }

  @override
  void onPoll() => unawaited(_load());

  @override
  void dispose() {
    AppControllers.engine.removeListener(_load);
    super.dispose();
  }

  Future<void> _load() async {
    final peers = await AppControllers.engine.peoplePeers();
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
                    GridView.builder(
                      shrinkWrap: true,
                      physics: const NeverScrollableScrollPhysics(),
                      gridDelegate:
                          const SliverGridDelegateWithMaxCrossAxisExtent(
                        maxCrossAxisExtent: 360,
                        crossAxisSpacing: 8,
                        mainAxisSpacing: 8,
                        mainAxisExtent: 154,
                      ),
                      itemCount: connections.length,
                      itemBuilder: (context, index) => PeerCard(
                        peer: connections[index],
                        contextLabel: connections[index].collectionName,
                      ),
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
          collectionId: entry.collections.join(','),
          collectionName: entry.collections
              .map((id) => names[id] ?? 'Unknown collection')
              .join(' · '),
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
