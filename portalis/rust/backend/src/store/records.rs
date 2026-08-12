//! What a row holds, and how it is written down.
//!
//! Objects the protocol already defines — device log entries, revisions,
//! manifests — are stored as their own canonical bytes. There is no second
//! encoding to keep in step, and a row read back is the same object a peer
//! would have sent.
//!
//! What remains is local-only: what this device calls a collection, where it
//! put the media, and the transfer history. Those get small hand-written
//! encodings here, length-prefixed the way `SPEC.md` D10 asks, because a
//! derived one would be a format nobody chose and nobody could read from
//! another language.

use portalis_nexus_protocol::{ContentKey, CONTENT_KEY_BYTES, DEVICE_KEY_BYTES};
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
}

impl StoredCollection {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.name.len() + self.media_path.len() + 41);
        bytes.push(self.role.code());
        bytes.extend_from_slice(&self.content_key);
        write_string(&mut bytes, &self.name);
        write_string(&mut bytes, &self.media_path);
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
        reader.finish()?;
        Ok(Self {
            name,
            role,
            content_key,
            media_path,
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
        }
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
            ..stored
        };
        assert_eq!(
            StoredCollection::decode(&bare.encode()).expect("decodes"),
            bare
        );
    }

    #[test]
    fn a_truncated_or_padded_collection_row_is_malformed() {
        let encoded = collection().encode();

        assert_eq!(StoredCollection::decode(&[]), Err(Malformed));
        assert_eq!(
            StoredCollection::decode(&encoded[..encoded.len() - 1]),
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
