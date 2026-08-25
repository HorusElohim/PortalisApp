//! What a row holds, and how it is written down.
//!
//! Objects the protocol already defines — device log entries, revisions,
//! manifests — are stored as their own canonical bytes. There is no second
//! encoding to keep in step, and a row read back is the same object a peer
//! would have sent.
//!
//! What remains is local-only: what this device calls a collection, where it
//! put the media, and the transfer history. Those get small hand-written
//! encodings here, length-prefixed, because a
//! derived one would be a format nobody chose and nobody could read from
//! another language.

use portalis_nexus_protocol::{CONTENT_KEY_BYTES, ContentKey, DEVICE_KEY_BYTES};
use thiserror::Error;

/// A row that does not decode. Always a bug or a damaged file, never input
/// from anywhere untrusted, so it needs no more detail than this.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("a stored row is malformed")]
pub struct Malformed;

/// Whether this device owns a collection or was given it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// We publish revisions and hold the content key's authority.
    Owner,
    /// We verify and read.
    Member,
}

impl Role {
    const OWNER: u8 = 1;
    const MEMBER: u8 = 2;

    const fn code(self) -> u8 {
        match self {
            Self::Owner => Self::OWNER,
            Self::Member => Self::MEMBER,
        }
    }

    const fn from_code(code: u8) -> Result<Self, Malformed> {
        match code {
            Self::OWNER => Ok(Self::Owner),
            Self::MEMBER => Ok(Self::Member),
            _ => Err(Malformed),
        }
    }
}

/// What this device knows about one collection that no peer needs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCollection {
    pub name: String,
    pub role: Role,
    /// The key every revision of this collection is sealed under.
    pub content_key: ContentKey,
    /// Where the media lives on this device. Chosen by the user, so it differs
    /// per device and never travels.
    pub media_path: String,
    /// Original no-copy sources while this device prepares and seeds the
    /// collection. Local-only; publications contain descriptors, not paths.
    pub sources: Vec<StoredSourceFile>,
    /// Whether this device has been told to stop transferring this
    /// collection. A local choice, like `media_path`: pausing on a phone says
    /// nothing about what a laptop should do, so it never travels.
    pub paused: bool,
    /// The substrate handle this collection is carried under, once it has
    /// one: the hex info hash of its torrent.
    ///
    /// Without it a holding cannot be attributed back to a collection, so
    /// every transfer number the interface shows would be an orphan. Local,
    /// like the rest of this record — the same collection on another device
    /// is carried under the same handle, but that device records it itself.
    pub substrate_handle: Option<String>,
    /// How many bytes of this collection this device is holding.
    ///
    /// Counted as it changes rather than measured on demand. The interface
    /// renders it on every snapshot, and walking a media directory to answer
    /// that is a filesystem scan per frame.
    pub on_disk_bytes: u64,
    /// When bytes first moved for this collection, and when it finished.
    ///
    /// Recorded rather than derived. The interface used to answer "completed
    /// in" by measuring the span of the transfer history, which is the span of
    /// whatever readings happened to survive the ring — after a delete and a
    /// re-add that read as one download lasting six minutes when it had been
    /// two downloads of thirty seconds. A time the engine wrote down when it
    /// happened cannot be re-measured into something else.
    ///
    /// Unix nanoseconds, and `None` until the moment each describes arrives.
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    /// Still being assembled, and not to be acted on yet.
    ///
    /// A person choosing files has not yet said to share them. Publishing at
    /// the moment of choosing means hashing and seeding something they may
    /// still rename, add to, or abandon — and abandoning it would then mean
    /// unpublishing something strangers may already hold. Held as a flag on
    /// the collection rather than as a separate kind of record, because a
    /// draft is the same thing before its first revision: the publisher simply
    /// leaves it alone until it is confirmed.
    pub draft: bool,
}

impl StoredCollection {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let source_bytes = self
            .sources
            .iter()
            .map(|source| source.label.len() + source.path.len() + 16)
            .sum::<usize>();
        let mut bytes =
            Vec::with_capacity(self.name.len() + self.media_path.len() + source_bytes + 45);
        bytes.push(self.role.code());
        bytes.extend_from_slice(&self.content_key);
        write_string(&mut bytes, &self.name);
        write_string(&mut bytes, &self.media_path);
        let count = u32::try_from(self.sources.len()).unwrap_or(u32::MAX);
        bytes.extend_from_slice(&count.to_be_bytes());
        for source in self.sources.iter().take(count as usize) {
            source.encode_into(&mut bytes);
        }
        bytes.push(u8::from(self.paused));
        bytes.extend_from_slice(&self.on_disk_bytes.to_be_bytes());
        write_string(&mut bytes, self.substrate_handle.as_deref().unwrap_or(""));
        bytes.push(u8::from(self.draft));
        // Zero stands for absent: a moment nothing has reached yet, and the
        // Unix epoch is not a time any of these can honestly have.
        bytes.extend_from_slice(&self.started_at.unwrap_or(0).to_be_bytes());
        bytes.extend_from_slice(&self.completed_at.unwrap_or(0).to_be_bytes());
        bytes
    }

    /// # Errors
    ///
    /// Returns [`Malformed`] when the row is truncated, carries trailing
    /// bytes, or names a role this version does not know.
    pub fn decode(bytes: &[u8]) -> Result<Self, Malformed> {
        let mut reader = Reader::new(bytes);
        let role = Role::from_code(reader.byte()?)?;
        let content_key = reader.array::<CONTENT_KEY_BYTES>()?;
        let name = reader.string()?;
        let media_path = reader.string()?;
        // Schemas 1–4 ended after `media_path`; they decode as collections
        // with no pending native sources.
        let sources = if reader.bytes.is_empty() {
            Vec::new()
        } else {
            let count = reader.u32()?;
            (0..count)
                .map(|_| StoredSourceFile::decode_from(&mut reader))
                .collect::<Result<Vec<_>, Malformed>>()?
        };
        // Schema 5 ended after the sources. Those rows decode as a collection
        // nobody has paused, holding nothing counted yet — which is what a
        // store written before the count existed can honestly claim.
        let (paused, on_disk_bytes) = if reader.bytes.is_empty() {
            (false, 0)
        } else {
            (reader.byte()? != 0, reader.u64()?)
        };
        // Schema 6 ended there. An empty handle and an absent one are the same
        // thing — a collection nothing is carrying yet — so they share a
        // representation rather than needing a flag to tell them apart.
        let substrate_handle = if reader.bytes.is_empty() {
            None
        } else {
            Some(reader.string()?).filter(|handle| !handle.is_empty())
        };
        // Schema 7 ended there. Everything written before drafts existed was
        // published the moment it was created, so a row without the flag is
        // exactly what `false` means — never a draft left in limbo.
        let draft = if reader.bytes.is_empty() {
            false
        } else {
            reader.byte()? != 0
        };
        // Schema 8 ended after the draft flag. Rows written before these
        // existed have no moments to report, which is what `None` says.
        let (started_at, completed_at) = if reader.bytes.is_empty() {
            (None, None)
        } else {
            (
                Some(reader.u64()?).filter(|at| *at != 0),
                Some(reader.u64()?).filter(|at| *at != 0),
            )
        };
        reader.finish()?;
        Ok(Self {
            name,
            role,
            content_key,
            media_path,
            sources,
            paused,
            on_disk_bytes,
            substrate_handle,
            draft,
            started_at,
            completed_at,
        })
    }
}

/// Where one entry's media has got to on this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStatus {
    /// The descriptor is held; nothing has been fetched.
    Known,
    Fetching,
    Available,
}

impl EntryStatus {
    const fn code(self) -> u8 {
        match self {
            Self::Known => 1,
            Self::Fetching => 2,
            Self::Available => 3,
        }
    }

    const fn from_code(code: u8) -> Result<Self, Malformed> {
        match code {
            1 => Ok(Self::Known),
            2 => Ok(Self::Fetching),
            3 => Ok(Self::Available),
            _ => Err(Malformed),
        }
    }
}

/// One entry's `.torrent` and what this device has done with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEntry {
    pub status: EntryStatus,
    pub descriptor: Vec<u8>,
}

impl StoredEntry {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.descriptor.len() + 1);
        bytes.push(self.status.code());
        bytes.extend_from_slice(&self.descriptor);
        bytes
    }

    /// # Errors
    ///
    /// Returns [`Malformed`] for an empty row or an unknown status.
    pub fn decode(bytes: &[u8]) -> Result<Self, Malformed> {
        let (&status, descriptor) = bytes.split_first().ok_or(Malformed)?;
        Ok(Self {
            status: EntryStatus::from_code(status)?,
            descriptor: descriptor.to_vec(),
        })
    }
}

/// One selectable file resolved from an imported torrent descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredImportEntry {
    pub label: String,
    pub bytes: u64,
    /// All resolved files start selected. The person's later selection is
    /// durable so restarting before confirmation cannot widen a download.
    pub selected: bool,
    /// The platform-owned location after a completed received file is moved
    /// out of Portalis' download folder. Local-only: an iOS `phasset://` or a
    /// future Android `content://` URI never leaves this device.
    pub native_location: Option<String>,
}

/// One original file selected for an owner-created collection.
///
/// Its path is local-only and durable because hashing can outlive the process
/// that accepted the command. The published descriptor contains names and
/// lengths, never these device-specific locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSourceFile {
    pub label: String,
    pub path: String,
    pub bytes: u64,
}

impl StoredSourceFile {
    fn encode_into(&self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&self.bytes.to_be_bytes());
        write_string(bytes, &self.label);
        write_string(bytes, &self.path);
    }

    fn decode_from(reader: &mut Reader<'_>) -> Result<Self, Malformed> {
        Ok(Self {
            bytes: reader.u64()?,
            label: reader.string()?,
            path: reader.string()?,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.label.len() + self.path.len() + 16);
        self.encode_into(&mut bytes);
        bytes
    }

    /// # Errors
    ///
    /// Returns [`Malformed`] when the row is truncated or carries trailing
    /// bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, Malformed> {
        let mut reader = Reader::new(bytes);
        let source = Self::decode_from(&mut reader)?;
        reader.finish()?;
        Ok(source)
    }
}

impl StoredImportEntry {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.label.len() + 13);
        bytes.extend_from_slice(&self.bytes.to_be_bytes());
        write_string(&mut bytes, &self.label);
        bytes.push(u8::from(self.selected));
        bytes.push(u8::from(self.native_location.is_some()));
        if let Some(location) = &self.native_location {
            write_string(&mut bytes, location);
        }
        bytes
    }

    /// # Errors
    ///
    /// Returns [`Malformed`] when the row is truncated or carries trailing
    /// bytes. Rows written before selection became explicit end after the
    /// label and mean every file was selected, preserving the safe default.
    pub fn decode(bytes: &[u8]) -> Result<Self, Malformed> {
        let mut reader = Reader::new(bytes);
        let bytes = reader.u64()?;
        let label = reader.string()?;
        let selected = if reader.bytes.is_empty() {
            true
        } else {
            reader.byte()? != 0
        };
        // Rows written before native gallery ownership existed end after the
        // selection bit. They remain app-folder downloads until a new move.
        let native_location = if reader.bytes.is_empty() {
            None
        } else if reader.byte()? == 0 {
            None
        } else {
            Some(reader.string()?)
        };
        reader.finish()?;
        Ok(Self {
            label,
            bytes,
            selected,
            native_location,
        })
    }
}

/// One reading of a transfer, at one moment.
///
/// Fixed width on purpose (§13): the history is a ring of these, and rows that
/// are all the same size can be counted and trimmed without decoding any of
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredSample {
    pub done: u64,
    pub total: u64,
    pub down_bytes_per_second: u32,
    pub up_bytes_per_second: u32,
    pub peers: u16,
}

impl StoredSample {
    /// 8 + 8 + 4 + 4 + 2.
    pub const BYTES: usize = 26;

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::BYTES);
        bytes.extend_from_slice(&self.done.to_be_bytes());
        bytes.extend_from_slice(&self.total.to_be_bytes());
        bytes.extend_from_slice(&self.down_bytes_per_second.to_be_bytes());
        bytes.extend_from_slice(&self.up_bytes_per_second.to_be_bytes());
        bytes.extend_from_slice(&self.peers.to_be_bytes());
        bytes
    }

    /// # Errors
    ///
    /// Returns [`Malformed`] unless the row is exactly [`Self::BYTES`] long.
    pub fn decode(bytes: &[u8]) -> Result<Self, Malformed> {
        let mut reader = Reader::new(bytes);
        let sample = Self {
            done: reader.u64()?,
            total: reader.u64()?,
            down_bytes_per_second: reader.u32()?,
            up_bytes_per_second: reader.u32()?,
            peers: u16::from_be_bytes(reader.array::<2>()?),
        };
        reader.finish()?;
        Ok(sample)
    }
}

/// A person this device knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredContact {
    pub handle: String,
    /// Whether the fingerprint has actually been compared (D4). A contact who
    /// has not been verified is shown as such rather than quietly trusted.
    pub fingerprint_verified: bool,
    pub root_key: [u8; DEVICE_KEY_BYTES],
}

impl StoredContact {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.handle.len() + 37);
        bytes.push(u8::from(self.fingerprint_verified));
        bytes.extend_from_slice(&self.root_key);
        write_string(&mut bytes, &self.handle);
        bytes
    }

    /// # Errors
    ///
    /// Returns [`Malformed`] when the row is truncated or carries trailing
    /// bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, Malformed> {
        let mut reader = Reader::new(bytes);
        let fingerprint_verified = reader.byte()? != 0;
        let root_key = reader.array::<DEVICE_KEY_BYTES>()?;
        let handle = reader.string()?;
        reader.finish()?;
        Ok(Self {
            handle,
            fingerprint_verified,
            root_key,
        })
    }
}

fn write_string(bytes: &mut Vec<u8>, value: &str) {
    // Saturating rather than panicking: a name long enough to overflow a u32
    // is a bug elsewhere, and truncating the length would corrupt the row
    // silently while this at least fails to decode.
    let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

/// A cursor that refuses to read past the end rather than panicking.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Malformed> {
        if self.bytes.len() < count {
            return Err(Malformed);
        }
        let (taken, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Malformed> {
        <[u8; N]>::try_from(self.take(N)?).map_err(|_| Malformed)
    }

    fn byte(&mut self) -> Result<u8, Malformed> {
        Ok(self.array::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, Malformed> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, Malformed> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn string(&mut self) -> Result<String, Malformed> {
        let length = self.u32()? as usize;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|_| Malformed)
    }

    /// Trailing bytes mean the row was written by something that disagrees
    /// about its shape, which is worth catching rather than ignoring.
    fn finish(&self) -> Result<(), Malformed> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Malformed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection() -> StoredCollection {
        StoredCollection {
            name: "Iceland, 2019".to_owned(),
            role: Role::Owner,
            content_key: [7; CONTENT_KEY_BYTES],
            media_path: "/Users/ada/Pictures/Iceland".to_owned(),
            sources: Vec::new(),
            paused: false,
            on_disk_bytes: 0,
            substrate_handle: None,
            draft: false,
            started_at: None,
            completed_at: None,
        }
    }

    /// A row written before schema 6 has no pause flag and no byte count. It
    /// decodes as a collection nobody paused holding nothing counted, rather
    /// than being refused — the fields are local facts, and inventing either
    /// would be worse than admitting the row predates them.
    #[test]
    fn a_row_from_before_the_local_facts_decodes_without_them() {
        let stored = StoredCollection {
            paused: true,
            on_disk_bytes: 4096,
            substrate_handle: Some("a1b2c3".to_owned()),
            ..collection()
        };
        let encoded = stored.encode();
        assert_eq!(
            StoredCollection::decode(&encoded).expect("decodes"),
            stored,
            "what was written is what comes back"
        );

        // The same row as schema 5 wrote it: everything up to the sources, so
        // without the pause flag, the byte count, the handle, the draft flag
        // or the two moments that each later schema appended.
        let older = &encoded[..encoded.len() - 16 - 1 - 9 - (4 + "a1b2c3".len())];
        let decoded = StoredCollection::decode(older).expect("an older row still decodes");
        assert!(!decoded.paused);
        assert_eq!(decoded.on_disk_bytes, 0);
        assert_eq!(decoded.substrate_handle, None);
        assert_eq!(decoded.name, stored.name);
    }

    #[test]
    fn a_collection_round_trips_including_an_empty_name_and_path() {
        let stored = collection();
        assert_eq!(
            StoredCollection::decode(&stored.encode()).expect("decodes"),
            stored
        );

        let bare = StoredCollection {
            name: String::new(),
            role: Role::Member,
            media_path: String::new(),
            ..stored.clone()
        };
        assert_eq!(
            StoredCollection::decode(&bare.encode()).expect("decodes"),
            bare
        );

        let with_sources = StoredCollection {
            sources: vec![StoredSourceFile {
                label: "Episode 1.mp4".to_owned(),
                path: "phasset://native-identifier".to_owned(),
                bytes: 42,
            }],
            ..collection()
        };
        assert_eq!(
            StoredCollection::decode(&with_sources.encode()).expect("decodes sources"),
            with_sources
        );

        let current = stored.encode();
        // What each later schema appended: a source count with no sources,
        // then the pause flag and the byte count. Removing both leaves the row
        // exactly as schemas 1 to 4 wrote it. Named rather than counted, so
        // the next field to arrive breaks this loudly instead of quietly
        // truncating somewhere else.
        const SOURCE_COUNT: usize = 4;
        const LOCAL_FACTS: usize = 1 + 8;
        const ABSENT_HANDLE: usize = 4;
        const DRAFT_FLAG: usize = 1;
        const MOMENTS: usize = 8 + 8;
        let schema_four = &current
            [..current.len() - SOURCE_COUNT - LOCAL_FACTS - ABSENT_HANDLE - DRAFT_FLAG - MOMENTS];
        assert_eq!(
            StoredCollection::decode(schema_four)
                .expect("older rows decode without invented sources")
                .sources,
            Vec::new()
        );
    }

    #[test]
    fn a_truncated_or_padded_collection_row_is_malformed() {
        let encoded = collection().encode();

        assert_eq!(StoredCollection::decode(&[]), Err(Malformed));
        // Two bytes, not one. Every schema appends, so the final byte is
        // always some older schema's absent field — dropping exactly it is a
        // valid older row rather than a damaged one, and asserting otherwise
        // would mean no field could ever be added again.
        assert_eq!(
            StoredCollection::decode(&encoded[..encoded.len() - 2]),
            Err(Malformed)
        );

        let mut padded = encoded.clone();
        padded.push(0);
        assert_eq!(StoredCollection::decode(&padded), Err(Malformed));

        let mut unknown_role = encoded.clone();
        unknown_role[0] = 9;
        assert_eq!(StoredCollection::decode(&unknown_role), Err(Malformed));

        // A length prefix that promises more than the row holds.
        let mut lying = encoded;
        lying[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(StoredCollection::decode(&lying), Err(Malformed));
    }

    #[test]
    fn a_name_that_is_not_utf8_is_malformed() {
        let mut row = Vec::new();
        row.push(Role::Owner.code());
        row.extend_from_slice(&[0; CONTENT_KEY_BYTES]);
        row.extend_from_slice(&2_u32.to_be_bytes());
        row.extend_from_slice(&[0xff, 0xfe]);
        row.extend_from_slice(&0_u32.to_be_bytes());

        assert_eq!(StoredCollection::decode(&row), Err(Malformed));
    }

    #[test]
    fn both_roles_survive_a_round_trip() {
        for role in [Role::Owner, Role::Member] {
            let stored = StoredCollection {
                role,
                ..collection()
            };
            assert_eq!(
                StoredCollection::decode(&stored.encode())
                    .expect("decodes")
                    .role,
                role
            );
        }
    }

    #[test]
    fn an_entry_keeps_its_descriptor_and_status() {
        for status in [
            EntryStatus::Known,
            EntryStatus::Fetching,
            EntryStatus::Available,
        ] {
            let stored = StoredEntry {
                status,
                descriptor: b"d8:announce0:e".to_vec(),
            };
            assert_eq!(
                StoredEntry::decode(&stored.encode()).expect("decodes"),
                stored
            );
        }

        // A descriptor may be empty; a row may not.
        let empty = StoredEntry {
            status: EntryStatus::Known,
            descriptor: Vec::new(),
        };
        assert_eq!(
            StoredEntry::decode(&empty.encode()).expect("decodes"),
            empty
        );
        assert_eq!(StoredEntry::decode(&[]), Err(Malformed));
        assert_eq!(StoredEntry::decode(&[9]), Err(Malformed));
    }

    #[test]
    fn an_imported_file_keeps_its_selection_and_native_gallery_location() {
        let selected = StoredImportEntry {
            label: "episode.mp4".to_owned(),
            bytes: 34,
            selected: true,
            native_location: Some("phasset://A1B2C3/L0/001".to_owned()),
        };
        let skipped = StoredImportEntry {
            selected: false,
            ..selected.clone()
        };
        assert_eq!(StoredImportEntry::decode(&selected.encode()), Ok(selected));
        assert_eq!(StoredImportEntry::decode(&skipped.encode()), Ok(skipped));

        // Version 3 stored the byte count and label only. It meant every
        // resolved file was selected, which remains the safe interpretation.
        let mut version_three = Vec::new();
        version_three.extend_from_slice(&34_u64.to_be_bytes());
        write_string(&mut version_three, "episode.mp4");
        assert_eq!(
            StoredImportEntry::decode(&version_three),
            Ok(StoredImportEntry {
                label: "episode.mp4".to_owned(),
                bytes: 34,
                selected: true,
                native_location: None,
            })
        );
    }

    #[test]
    fn a_zero_copy_source_keeps_its_native_location_and_metadata() {
        let source = StoredSourceFile {
            label: "Episode 1.mp4".to_owned(),
            path: "phasset://native-identifier".to_owned(),
            bytes: 42,
        };

        assert_eq!(StoredSourceFile::decode(&source.encode()), Ok(source));
        assert_eq!(StoredSourceFile::decode(&[]), Err(Malformed));
    }

    #[test]
    fn a_sample_is_exactly_one_fixed_width_row() {
        let sample = StoredSample {
            done: 1_024,
            total: 4_096,
            down_bytes_per_second: 512,
            up_bytes_per_second: 128,
            peers: 3,
        };
        let encoded = sample.encode();

        assert_eq!(encoded.len(), StoredSample::BYTES);
        assert_eq!(StoredSample::decode(&encoded).expect("decodes"), sample);
        assert_eq!(
            StoredSample::decode(&encoded[..StoredSample::BYTES - 1]),
            Err(Malformed)
        );

        let mut padded = encoded;
        padded.push(0);
        assert_eq!(StoredSample::decode(&padded), Err(Malformed));
    }

    #[test]
    fn a_contact_remembers_whether_its_fingerprint_was_compared() {
        for fingerprint_verified in [true, false] {
            let contact = StoredContact {
                handle: "ada#7Q2XZ".to_owned(),
                fingerprint_verified,
                root_key: [3; DEVICE_KEY_BYTES],
            };
            assert_eq!(
                StoredContact::decode(&contact.encode()).expect("decodes"),
                contact
            );
        }

        assert_eq!(StoredContact::decode(&[]), Err(Malformed));
        assert_eq!(StoredContact::decode(&[1; 4]), Err(Malformed));
    }

    /// A name too long to describe is written with a length that cannot
    /// describe it, so the row fails to decode rather than silently losing
    /// its tail.
    #[test]
    fn a_length_that_cannot_be_written_fails_to_decode_rather_than_truncating() {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "short");

        assert_eq!(&bytes[..4], &5_u32.to_be_bytes());
        assert_eq!(&bytes[4..], b"short");
    }
}
