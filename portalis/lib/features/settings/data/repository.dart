import '../../../nexus/bridge/portalis_api.dart' as nexus_bridge;
import '../../../nexus/bridge/settings.dart' as settings_bridge;
import '../domain/engine.dart';
import '../domain/storage_entry.dart';

/// Native operations needed by the settings feature.
abstract interface class SettingsRepository {
  Future<EngineSettings> load();
  Future<EngineSettings> defaults();
  Future<bool> save(EngineSettings settings);
  Future<int> storageUsageBytes();
  Future<List<StorageEntry>> storageBreakdown();
}

/// Flutter-Rust Bridge implementation. Generated types stay in this file.
class FrbSettingsRepository implements SettingsRepository {
  const FrbSettingsRepository();

  @override
  Future<EngineSettings> load() async =>
      _fromBridge(await settings_bridge.engineSettings());

  @override
  Future<EngineSettings> defaults() async =>
      _fromBridge(await settings_bridge.defaultEngineSettings());

  @override
  Future<bool> save(EngineSettings settings) =>
      settings_bridge.setEngineSettings(settings: _toBridge(settings));

  @override
  Future<int> storageUsageBytes() async {
    // ADR-0001: the single seam is `portalis_api`. The total is the sum of
    // the per-entry breakdown that seam already exposes, so no second
    // bridge function is needed for the meter.
    final entries = await nexus_bridge.storageBreakdown();
    return entries.fold<int>(0, (total, entry) => total + entry.bytes.toInt());
  }

  @override
  Future<List<StorageEntry>> storageBreakdown() async =>
      (await nexus_bridge.storageBreakdown())
          .map(
            (entry) => StorageEntry(
              name: entry.name,
              bytes: entry.bytes.toInt(),
              path: entry.path,
              collection: entry.collection,
              collectionName: entry.collectionName,
            ),
          )
          .toList(growable: false);

  static EngineSettings _fromBridge(settings_bridge.EngineSettings settings) =>
      EngineSettings(
        uploadLimitBps: settings.uploadLimitBps,
        downloadLimitBps: settings.downloadLimitBps,
        downloadDir: settings.downloadDir,
        listenPortStart: settings.listenPortStart,
        listenPortEnd: settings.listenPortEnd,
        enableUpnpPortForwarding: settings.enableUpnpPortForwarding,
        socksProxyUrl: settings.socksProxyUrl,
        disableDht: settings.disableDht,
        disableDhtPersistence: settings.disableDhtPersistence,
        persistSession: settings.persistSession,
        fastresume: settings.fastresume,
        deferWritesUpToMb: settings.deferWritesUpToMb,
        concurrentInitLimit: settings.concurrentInitLimit,
        peerConnectTimeoutSecs: settings.peerConnectTimeoutSecs,
        peerReadWriteTimeoutSecs: settings.peerReadWriteTimeoutSecs,
        peerKeepAliveIntervalSecs: settings.peerKeepAliveIntervalSecs,
        blocklistUrl: settings.blocklistUrl,
        trackers: List.unmodifiable(settings.trackers),
      );

  static settings_bridge.EngineSettings _toBridge(EngineSettings settings) =>
      settings_bridge.EngineSettings(
        uploadLimitBps: settings.uploadLimitBps,
        downloadLimitBps: settings.downloadLimitBps,
        downloadDir: settings.downloadDir,
        listenPortStart: settings.listenPortStart,
        listenPortEnd: settings.listenPortEnd,
        enableUpnpPortForwarding: settings.enableUpnpPortForwarding,
        socksProxyUrl: settings.socksProxyUrl,
        disableDht: settings.disableDht,
        disableDhtPersistence: settings.disableDhtPersistence,
        persistSession: settings.persistSession,
        fastresume: settings.fastresume,
        deferWritesUpToMb: settings.deferWritesUpToMb,
        concurrentInitLimit: settings.concurrentInitLimit,
        peerConnectTimeoutSecs: settings.peerConnectTimeoutSecs,
        peerReadWriteTimeoutSecs: settings.peerReadWriteTimeoutSecs,
        peerKeepAliveIntervalSecs: settings.peerKeepAliveIntervalSecs,
        blocklistUrl: settings.blocklistUrl,
        trackers: settings.trackers,
      );
}
