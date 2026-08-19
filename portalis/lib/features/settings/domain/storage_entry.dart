/// One top-level item under the configured download folder.
class StorageEntry {
  const StorageEntry({
    required this.name,
    required this.bytes,
    required this.path,
    this.collection,
    this.collectionName,
  });

  final String name;
  final int bytes;
  final String path;

  /// The owning collection's Nexus handle, when one claims this directory.
  final int? collection;
  final String? collectionName;
}
