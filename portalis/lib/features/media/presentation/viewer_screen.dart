import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:video_player/video_player.dart';

import '../../../design/design.dart';
import '../../../nexus/application/app_controller.dart';
import '../../../nexus/domain/app_state.dart';
import '../application/formats.dart';
import 'viewer.dart';

/// Views one generated entry while subscribing only to its collection detail.
class MediaViewerScreen extends StatefulWidget {
  const MediaViewerScreen({
    super.key,
    required this.controller,
    required this.collectionId,
    required this.entryId,
  });

  final AppController controller;
  final int collectionId;
  final int entryId;

  @override
  State<MediaViewerScreen> createState() => _MediaViewerScreenState();
}

class _MediaViewerScreenState extends State<MediaViewerScreen> {
  AppDetail? _detail;
  StreamSubscription<AppDetail?>? _detailSubscription;
  VideoPlayerController? _videoController;
  String? _playingPath;
  bool _videoFailed = false;

  AppCollection? get _collection => widget.controller.state?.collections
      .where((item) => item.id == widget.collectionId)
      .firstOrNull;

  AppEntry? get _entry =>
      _detail?.entries.where((item) => item.id == widget.entryId).firstOrNull;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_syncFromController);
    _detailSubscription =
        widget.controller.watchDetail(widget.collectionId).listen((detail) {
      if (!mounted) return;
      setState(() => _detail = detail);
      _syncVideo();
    });
  }

  void _syncFromController() {
    if (mounted) setState(_syncVideo);
  }

  @override
  void dispose() {
    widget.controller.removeListener(_syncFromController);
    unawaited(_detailSubscription?.cancel());
    _disposeVideo();
    super.dispose();
  }

  bool get _isPlayableVideo {
    final entry = _entry;
    return entry != null &&
        entry.isReady &&
        MediaFormats.resolve(entry.label).preview == PreviewSupport.player;
  }

  void _syncVideo() {
    final path = _isPlayableVideo ? _entry?.localPath : null;
    if (path == _playingPath) return;
    _disposeVideo();
    _playingPath = path;
    if (path != null) _initializeVideo(path);
  }

  void _initializeVideo(String path) {
    final controller = VideoPlayerController.file(File(path));
    _videoController = controller;
    controller.initialize().then((_) {
      if (!mounted || _videoController != controller) return;
      if (!controller.value.isInitialized) {
        setState(() => _videoFailed = true);
        return;
      }
      setState(() {});
    }).catchError((_) {
      if (mounted && _videoController == controller) {
        setState(() => _videoFailed = true);
      }
    });
  }

  void _disposeVideo() {
    _videoController?.dispose();
    _videoController = null;
    _playingPath = null;
    _videoFailed = false;
  }

  Future<void> _openExternally() async {
    final path = _entry?.localPath;
    if (path == null) return;
    final opened = await launchUrl(Uri.file(path));
    if (!opened && mounted) {
      showToast(context, 'Couldn\'t open ${_entry?.label ?? 'file'}',
          severity: ToastSeverity.error);
    }
  }

  void _refreshVideo() {
    _disposeVideo();
    _syncVideo();
  }

  @override
  Widget build(BuildContext context) {
    final collection = _collection;
    final entry = _entry;
    if (collection == null || entry == null) {
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }
    return CollectionMediaViewer(
      collection: collection,
      media: entry,
      isPlayableVideo: _isPlayableVideo,
      videoFailed: _videoFailed,
      videoController: _videoController,
      onClose: () => Navigator.of(context).pop(),
      onRefresh: _refreshVideo,
      onOpenExternally: _openExternally,
    );
  }
}
