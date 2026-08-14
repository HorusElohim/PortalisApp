import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../application/nexus_app_controller.dart';
import '../domain/nexus_app_state.dart';

/// Lets a person inspect a Nexus-imported torrent before payload download.
///
/// The stream is the backend's selection state. This widget keeps only an
/// in-progress checkbox edit; confirming hands that selection back as one
/// command and the next stream value becomes authoritative again.
class NexusTorrentPreparation extends StatefulWidget {
  const NexusTorrentPreparation({
    super.key,
    required this.collection,
    required this.controller,
  });

  final int collection;
  final NexusAppController controller;

  @override
  State<NexusTorrentPreparation> createState() =>
      _NexusTorrentPreparationState();
}

class _NexusTorrentPreparationState extends State<NexusTorrentPreparation> {
  Set<int>? _selected;
  bool _saving = false;

  Future<void> _confirm(NexusDetail detail) async {
    final selected = _selected ?? _selectedFrom(detail);
    if (selected.isEmpty) return;
    setState(() => _saving = true);
    try {
      await widget.controller.send(
        NexusCommand(
          kind: 'downloadSelection',
          collection: widget.collection,
          entries: selected.toList()..sort(),
        ),
      );
      if (mounted) {
        showToast(
          context,
          'Selection saved — download is waiting for the Nexus torrent substrate.',
        );
      }
    } catch (error) {
      if (mounted) showToast(context, '$error', severity: ToastSeverity.error);
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Set<int> _selectedFrom(NexusDetail detail) => {
        for (final entry in detail.entries)
          if (entry.selected) entry.id,
      };

  @override
  Widget build(BuildContext context) => AppScreen(
        title: 'Prepare download',
        subtitle: const Text('Choose files before any payload is requested.'),
        onBack: () => Navigator.of(context).maybePop(),
        body: StreamBuilder<NexusDetail?>(
          stream: widget.controller.watchDetail(widget.collection),
          builder: (context, snapshot) {
            final detail = snapshot.data;
            if (detail == null) {
              return const Center(child: CircularProgressIndicator());
            }
            if (detail.entries.isEmpty) {
              return Center(
                child: Padding(
                  padding: const EdgeInsets.all(kScreenGutter),
                  child: Text(
                    'Waiting for torrent metadata before you choose files.',
                    textAlign: TextAlign.center,
                    style: AppText.body(color: AppColors.textDim),
                  ),
                ),
              );
            }
            final selected = _selected ?? _selectedFrom(detail);
            final selectedBytes = detail.entries
                .where((entry) => selected.contains(entry.id))
                .fold<BigInt>(BigInt.zero, (sum, entry) => sum + entry.bytes);
            return SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(
                  kScreenGutter, 0, kScreenGutter, 28),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '${detail.entries.length} file${detail.entries.length == 1 ? '' : 's'} · '
                    '${formatBytes(selectedBytes.toInt())} selected',
                    style: monoLabel(size: 11, color: AppColors.textDim),
                  ),
                  const SizedBox(height: 12),
                  SurfaceCard(
                    child: Column(
                      children: [
                        for (final entry in detail.entries)
                          Material(
                            color: Colors.transparent,
                            child: CheckboxListTile(
                              key: Key('nexusTorrentEntry:${entry.id}'),
                              value: selected.contains(entry.id),
                              onChanged: _saving
                                  ? null
                                  : (checked) => setState(() {
                                        final next = Set<int>.of(selected);
                                        if (checked ?? false) {
                                          next.add(entry.id);
                                        } else {
                                          next.remove(entry.id);
                                        }
                                        _selected = next;
                                      }),
                              title: Text(
                                entry.label,
                                overflow: TextOverflow.ellipsis,
                                style: AppText.body(),
                              ),
                              subtitle: Text(
                                formatBytes(entry.bytes.toInt()),
                                style: monoLabel(
                                  size: 10,
                                  color: AppColors.textDim,
                                ),
                              ),
                              activeColor: AppColors.ember,
                              controlAffinity: ListTileControlAffinity.trailing,
                            ),
                          ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 16),
                  PrimaryActionButton(
                    key: const Key('nexusConfirmSelection'),
                    label: _saving
                        ? 'Saving selection…'
                        : 'Confirm ${selected.length} file${selected.length == 1 ? '' : 's'}',
                    icon: Icons.download_outlined,
                    expand: true,
                    tone: ActionButtonTone.ember,
                    onTap: _saving || selected.isEmpty
                        ? null
                        : () => _confirm(detail),
                  ),
                ],
              ),
            );
          },
        ),
      );
}
