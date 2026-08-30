import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:share_plus/share_plus.dart';

import '../../../app/app_controllers.dart';
import '../../../design/design.dart';
import '../../../design/theme.dart';

/// The local diagnostics log — plain text, read straight off disk.
///
/// This is deliberately not a telemetry screen. Nothing here is uploaded to
/// any server on its own: the log is written locally by the Rust backend
/// (`nexus::diagnostics`) beside the rest of this app's own state, and the
/// only way it ever leaves the device is a person tapping Share and picking
/// where it goes — email, a chat, a USB cable, whatever they choose. That is
/// the same freedom the rest of Portalis is built around: no company server
/// in the loop, no account, no automatic upload.
class DiagnosticsScreen extends StatefulWidget {
  const DiagnosticsScreen({super.key, this.embedded = false, this.onBack});

  /// Set when this replaces the Settings pane in place on desktop rather
  /// than being pushed over it — see [AppScreen].
  final bool embedded;

  /// Called instead of popping a route. Only meaningful when [embedded]:
  /// there is no route to pop there, so the caller supplies its own
  /// "collapse back to Settings" callback.
  final VoidCallback? onBack;

  @override
  State<DiagnosticsScreen> createState() => _DiagnosticsScreenState();
}

class _DiagnosticsScreenState extends State<DiagnosticsScreen>
    with PollingState {
  String? _log;
  String? _path;
  String? _error;

  @override
  void initState() {
    super.initState();
    // The log grows while this screen is open — a torrent finishing, a peer
    // connecting — so it stays live the same way Storage's breakdown does.
    startPolling();
  }

  @override
  void onPoll() => _load();

  Future<void> _load() async {
    try {
      final log = await AppControllers.engine.diagnosticsLog();
      final path = await AppControllers.engine.diagnosticsLogPath();
      if (!mounted) return;
      setState(() {
        _log = log;
        _path = path;
        _error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  Future<void> _share() async {
    final log = _log;
    final path = _path;
    if (log == null || log.isEmpty || path == null) {
      showToast(context, 'Nothing to share yet.');
      return;
    }
    try {
      // Shared directly from where the backend already writes it, rather
      // than copied into a second file first: getTemporaryDirectory()'s
      // path is not guaranteed to exist on every platform (it failed with
      // PathNotFoundException on macOS), and a copy is one more thing that
      // can go stale or go missing for no reason the log file itself
      // doesn't already have.
      await SharePlus.instance.share(
        ShareParams(
          files: [XFile(path)],
          subject: 'Portalis diagnostics',
          text: 'Portalis diagnostics log — shared from the device, not '
              'uploaded anywhere automatically.',
        ),
      );
    } catch (e) {
      if (!mounted) return;
      showToast(context, 'Couldn\'t share: $e', severity: ToastSeverity.error);
    }
  }

  Future<void> _copyPath() async {
    final path = _path;
    if (path == null || path.isEmpty) return;
    await Clipboard.setData(ClipboardData(text: path));
    if (!mounted) return;
    showToast(context, 'Path copied.');
  }

  Future<void> _confirmClear() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppColors.surface,
        title: const Text('Clear diagnostics log?'),
        content: Text(
          'Removes everything recorded so far. This only deletes the local '
          'file — nothing was ever sent anywhere.',
          style: AppText.secondary(color: AppColors.textDim),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Clear'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      await AppControllers.engine.clearDiagnosticsLog();
      await _load();
    } catch (e) {
      if (!mounted) return;
      showToast(context, 'Couldn\'t clear: $e', severity: ToastSeverity.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final log = _log;
    return AppScreen(
      title: 'Diagnostics',
      subtitle: Text(
        log == null
            ? 'Reading the log…'
            : log.isEmpty
                ? 'Nothing recorded yet.'
                : '${log.trimRight().split('\n').length} line'
                    '${log.trimRight().split('\n').length == 1 ? '' : 's'} '
                    '· kept only on this device',
      ),
      embedded: widget.embedded,
      forceShowBack: true,
      onBack: widget.onBack,
      // Same measure as Settings — see PageBody.settingsWideMaxWidth. This
      // was the screen that surfaced the inconsistency: reached from
      // Settings but still on the app's narrower default measure, it read
      // visibly cramped next to the screen it was opened from.
      wideMaxWidth: PageBody.settingsWideMaxWidth,
      footer: Row(
        children: [
          Expanded(
            child: OutlineActionButton(
              label: 'Clear',
              tone: ActionButtonTone.neutral,
              onTap: log == null || log.isEmpty ? null : _confirmClear,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            flex: 2,
            child: PrimaryActionButton(
              label: 'Share…',
              icon: Icons.ios_share,
              onTap: log == null || log.isEmpty ? null : _share,
            ),
          ),
        ],
      ),
      body: _body(log),
    );
  }

  Widget _body(String? log) {
    if (_error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(22),
          child: Text(
            _error!,
            textAlign: TextAlign.center,
            style: AppText.body(color: AppColors.danger),
          ),
        ),
      );
    }
    if (log == null) {
      return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    }
    return ListView(
      padding: const EdgeInsets.fromLTRB(kScreenGutter, 0, kScreenGutter, 24),
      children: [
        InfoBanner(
          color: AppColors.signalSoft,
          icon: Icons.lock_outline,
          text: 'Stays on this device. Nothing here is sent anywhere unless '
              'you tap Share.',
        ),
        const SizedBox(height: 12),
        if (_path != null)
          InkWell(
            onTap: _copyPath,
            borderRadius: BorderRadius.circular(AppRadius.inner),
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Row(
                children: [
                  Icon(Icons.insert_drive_file_outlined,
                      size: 13, color: AppColors.textFaint),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      _path!,
                      overflow: TextOverflow.ellipsis,
                      style: monoLabel(size: 10, color: AppColors.textFaint),
                    ),
                  ),
                  Icon(Icons.copy, size: 12, color: AppColors.textFaint),
                ],
              ),
            ),
          ),
        const SizedBox(height: 10),
        if (log.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 40),
            child: Center(
              child: Text(
                'Nothing recorded yet.',
                style: AppText.body(color: AppColors.textDim),
              ),
            ),
          )
        else
          SurfaceCard(
            padding: const EdgeInsets.all(14),
            child: SelectableText(
              log,
              style: monoLabel(size: 10.5, color: AppColors.textDim, letterSpacing: 0),
            ),
          ),
      ],
    );
  }
}
