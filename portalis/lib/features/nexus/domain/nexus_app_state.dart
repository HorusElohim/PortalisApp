/// App-owned Nexus projection types.
///
/// Widgets and controllers depend on these values, never directly on generated
/// Flutter-Rust Bridge classes. The adapter owns that conversion in one place.
class NexusAppState {
  const NexusAppState({
    required this.device,
    required this.connectivity,
    required this.contacts,
    required this.collections,
    required this.alerts,
  });

  final NexusDevice device;
  final String connectivity;
  final List<NexusContact> contacts;
  final List<NexusCollection> collections;
  final List<String> alerts;
}

class NexusDevice {
  const NexusDevice({
    required this.name,
    required this.handle,
    required this.fingerprint,
    required this.devices,
  });

  final String name;
  final String? handle;
  final String fingerprint;
  final int devices;
}

class NexusContact {
  const NexusContact({
    required this.id,
    required this.displayName,
    required this.handle,
    required this.fingerprint,
    required this.verified,
    required this.friendship,
    required this.reachable,
  });

  final int id;
  final String displayName;
  final String? handle;
  final String fingerprint;
  final bool verified;
  final String friendship;
  final String? reachable;
}

class NexusCollection {
  const NexusCollection({
    required this.id,
    required this.name,
    required this.role,
    required this.revision,
    required this.status,
    required this.members,
    required this.entries,
    required this.totalBytes,
    required this.transfer,
    required this.pending,
  });

  final int id;
  final String name;
  final String role;
  final BigInt revision;
  final String status;
  final List<int> members;
  final int entries;
  final BigInt totalBytes;
  final NexusTransfer? transfer;
  final NexusPending? pending;
}

class NexusTransfer {
  const NexusTransfer({
    required this.progress,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.peers,
    required this.etaSecs,
  });

  final double progress;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final int peers;
  final int? etaSecs;
}

class NexusPending {
  const NexusPending({required this.command, required this.queued});

  final BigInt command;
  final bool queued;
}

class NexusDetail {
  const NexusDetail({
    required this.id,
    required this.entries,
    required this.pieces,
    required this.samples,
  });

  final int id;
  final List<NexusEntry> entries;
  final List<int> pieces;
  final List<int> samples;
}

class NexusEntry {
  const NexusEntry({
    required this.id,
    required this.label,
    required this.bytes,
    required this.selected,
    required this.available,
  });

  final int id;
  final String label;
  final BigInt bytes;
  final bool selected;
  final bool available;
}

/// One command envelope. Command-specific fields remain optional so adding a
/// command does not make Dart duplicate the bridge's generated union code.
class NexusCommand {
  const NexusCommand({
    required this.kind,
    this.name,
    this.files = const [],
    this.collection,
    this.label,
    this.deleteFiles,
    this.entry,
    this.source,
    this.entries = const [],
    this.contact,
    this.handle,
    this.accept,
    this.device,
    this.active,
  });

  final String kind;
  final String? name;
  final List<String> files;
  final int? collection;
  final String? label;
  final bool? deleteFiles;
  final int? entry;
  final String? source;
  final List<int> entries;
  final int? contact;
  final String? handle;
  final bool? accept;
  final int? device;
  final bool? active;

  const NexusCommand.importTorrent(String source)
      : this(kind: 'importTorrent', source: source);
}

class NexusAccepted {
  const NexusAccepted({
    required this.id,
    required this.collection,
    required this.queued,
  });

  final BigInt id;
  final int? collection;
  final bool queued;
}
