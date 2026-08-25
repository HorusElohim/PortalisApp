//! Durable source descriptors for torrents that seed directly from a gallery.

use serde::{Deserialize, Serialize};

use crate::nexus::torrent::SourceFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LinkedSourceRecord {
    pub(crate) info_hash: String,
    pub(crate) torrent_bytes: Vec<u8>,
    pub(crate) sources: Vec<SourceFile>,
    /// Receiver collections may have intentionally unselected files whose
    /// filesystem destination does not exist yet. Their declared torrent
    /// lengths remain authoritative until a future selection writes them.
    #[serde(default)]
    pub(crate) allow_missing_files: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct LinkedSourceStore {
    records: Vec<LinkedSourceRecord>,
}

fn vault() -> crate::nexus::vault::Vault {
    crate::nexus::vault::Vault::named("linked-sources.json")
}

pub(crate) fn load() -> anyhow::Result<Vec<LinkedSourceRecord>> {
    Ok(vault()
        .read::<LinkedSourceStore>()?
        .unwrap_or_default()
        .records)
}

pub(crate) fn upsert(record: LinkedSourceRecord) -> anyhow::Result<()> {
    let mut records = load()?;
    records.retain(|existing| existing.info_hash != record.info_hash);
    records.push(record);
    vault().write(&LinkedSourceStore { records })
}

pub(crate) fn remove(info_hash: &str) -> anyhow::Result<()> {
    let mut records = load()?;
    records.retain(|record| record.info_hash != info_hash);
    vault().write(&LinkedSourceStore { records })
}

pub(crate) fn descriptor_for(info_hash: &str) -> anyhow::Result<Vec<u8>> {
    load()?
        .into_iter()
        .find(|record| record.info_hash.eq_ignore_ascii_case(info_hash))
        .map(|record| record.torrent_bytes)
        .ok_or_else(|| anyhow::anyhow!("no descriptor was persisted for {info_hash}"))
}

/// The original locations a referenced torrent reads, when this device owns
/// them. These paths are projection data, not a staging layout: a caller uses
/// them to preview the same source the storage adapter reads.
pub(crate) fn sources_for(info_hash: &str) -> anyhow::Result<Option<Vec<SourceFile>>> {
    Ok(load()?
        .into_iter()
        .find(|record| record.info_hash.eq_ignore_ascii_case(info_hash))
        .map(|record| record.sources))
}
