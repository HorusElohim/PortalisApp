import 'package:flutter/material.dart';

import '../../../app/app_controllers.dart';
import '../../../design/design.dart';
import '../../../nexus/domain/app_state.dart';
import '../../../design/theme.dart';

/// The people this device knows, from Nexus.
///
/// Contacts only. An anonymous swarm address is a property of a *transfer*,
/// not of a person: it carries no signed identity, so pooling addresses into
/// a directory of strangers would put them beside people this device holds
/// real evidence about. They live on the collection moving bytes with them
/// instead — see `Detail.peers`. Nexus keeps no record of departed addresses,
/// so there is nothing here to forget either.
class PeopleScreen extends StatelessWidget {
  const PeopleScreen({super.key, this.embedded = false});

  final bool embedded;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: AppControllers.nexusApp,
        builder: (context, _) => _build(context),
      );

  Widget _build(BuildContext context) {
    final state = AppControllers.nexusApp.state;
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

    return AppScreen(
      title: 'People',
      subtitle:
          entries.isEmpty ? null : Text(plural(entries.length, 'contact')),
      embedded: embedded,
      width: ScreenWidth.full,
      body: entries.isEmpty
          ? Center(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: kScreenGutter),
                child: Text(
                  'Nobody yet. People appear here once you add a contact, or '
                  'once somebody shares a collection with you.',
                  textAlign: TextAlign.center,
                  style: AppText.body(color: AppColors.textDim),
                ),
              ),
            )
          : WindowBuilder(
              builder: (context, window) => GridView.builder(
                padding: const EdgeInsets.fromLTRB(
                  kScreenGutter,
                  0,
                  kScreenGutter,
                  28,
                ),
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
            ),
    );
  }
}

/// One person this device holds signed statements about.
///
/// Shows no transfer figures. Neither engine attributes bytes to a signed
/// identity — a collection knows what it moved, not which of its members
/// consumed it — and the card this replaces showed a pooled collection total
/// wearing a person's name.
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
