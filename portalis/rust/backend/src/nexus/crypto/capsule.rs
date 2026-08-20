//! What a shared collection is, once it is small enough to send.
//!
//! Nexus stores a capsule as opaque bytes and cannot read one. This is the
//! agreement between the devices at either end about what those bytes mean:
//! enough to start downloading a collection, and nothing about who it came
//! from — that is on the envelope, which the service does read.
//!
//! It carries the torrent itself rather than a magnet link. A magnet is a
//! promise that the description can be found somewhere; the torrent is the
//! description. Since the recipient is about to be handed the addresses of
//! devices that already hold the files, needing DHT or a tracker first would
//! add a way to fail to a step that has no reason to have one.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use thiserror::Error;

use portalis_nexus_protocol::ContentKey;

const NONCE_BYTES: usize = 12;

/// A cap on what one capsule may describe, so a malformed or hostile one
/// cannot be turned into an allocation.
#[allow(dead_code)]
const MAX_CAPSULE_BYTES: usize = 8 * 1024 * 1024;

/// Long enough for any name a person types, short enough to bound the decode.
#[allow(dead_code)]
const MAX_NAME_BYTES: usize = 512;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CapsuleError {
    #[error("a capsule cannot be read without its content key")]
    NotForThisKey,
    #[error("this is not a capsule")]
    Malformed,
    #[error("a capsule describes at most {MAX_CAPSULE_BYTES} bytes, and this one claims more")]
    TooLarge,
    #[error("a collection name is at most {MAX_NAME_BYTES} bytes")]
    NameTooLong,
}

/// Everything a device needs to start receiving a collection it was given.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Capsule {
    /// What the collection is called, as its owner named it.
    pub name: String,
    /// The torrent describing its files, ready to hand to an engine.
    pub torrent: Vec<u8>,
}

impl Capsule {
    #[allow(dead_code)]
    pub(crate) fn seal(
        &self,
        key: &ContentKey,
        share_id: &[u8],
        revision: u64,
    ) -> Result<Vec<u8>, CapsuleError> {
        let plaintext = self.encode()?;
        let nonce = nonce();
        let ciphertext = ChaCha20Poly1305::new(&Key::from(*key))
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &plaintext,
                    aad: &associated_data(share_id, revision),
                },
            )
            .map_err(|_| CapsuleError::TooLarge)?;

        let mut sealed = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    #[allow(dead_code)]
    pub(crate) fn open(
        key: &ContentKey,
        share_id: &[u8],
        revision: u64,
        sealed: &[u8],
    ) -> Result<Self, CapsuleError> {
        if sealed.len() <= NONCE_BYTES {
            return Err(CapsuleError::Malformed);
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_BYTES);
        let plaintext = ChaCha20Poly1305::new(&Key::from(*key))
            .decrypt(
                &Nonce::from(
                    <[u8; NONCE_BYTES]>::try_from(nonce).map_err(|_| CapsuleError::Malformed)?,
                ),
                Payload {
                    msg: ciphertext,
                    aad: &associated_data(share_id, revision),
                },
            )
            .map_err(|_| CapsuleError::NotForThisKey)?;
        Self::decode(&plaintext)
    }

    #[allow(dead_code)]
    fn encode(&self) -> Result<Vec<u8>, CapsuleError> {
        let name = self.name.as_bytes();
        if name.len() > MAX_NAME_BYTES {
            return Err(CapsuleError::NameTooLong);
        }
        if self.torrent.len() > MAX_CAPSULE_BYTES {
            return Err(CapsuleError::TooLarge);
        }
        let mut bytes = Vec::with_capacity(6 + name.len() + self.torrent.len());
        bytes.extend_from_slice(&u16::try_from(name.len()).unwrap_or(u16::MAX).to_le_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(
            &u32::try_from(self.torrent.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&self.torrent);
        Ok(bytes)
    }

    #[allow(dead_code)]
    fn decode(bytes: &[u8]) -> Result<Self, CapsuleError> {
        let name_length = usize::from(u16::from_le_bytes(
            bytes
                .get(..2)
                .ok_or(CapsuleError::Malformed)?
                .try_into()
                .map_err(|_| CapsuleError::Malformed)?,
        ));
        let name_end = 2 + name_length;
        let name = bytes.get(2..name_end).ok_or(CapsuleError::Malformed)?;
        let torrent_length = u32::from_le_bytes(
            bytes
                .get(name_end..name_end + 4)
                .ok_or(CapsuleError::Malformed)?
                .try_into()
                .map_err(|_| CapsuleError::Malformed)?,
        ) as usize;
        if torrent_length > MAX_CAPSULE_BYTES {
            return Err(CapsuleError::TooLarge);
        }
        let torrent = bytes
            .get(name_end + 4..name_end + 4 + torrent_length)
            .ok_or(CapsuleError::Malformed)?;

        Ok(Self {
            name: String::from_utf8(name.to_vec()).map_err(|_| CapsuleError::Malformed)?,
            torrent: torrent.to_vec(),
        })
    }
}

#[allow(dead_code)]
pub(crate) fn nonce() -> [u8; NONCE_BYTES] {
    let mut nonce = [0_u8; NONCE_BYTES];
    nonce.copy_from_slice(&portalis_nexus_protocol::new_challenge()[..NONCE_BYTES]);
    nonce
}

#[allow(dead_code)]
pub(crate) fn associated_data(share_id: &[u8], revision: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(share_id.len() + 8);
    data.extend_from_slice(share_id);
    data.extend_from_slice(&revision.to_le_bytes());
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capsule() -> Capsule {
        Capsule {
            name: "Iceland, 2019".to_owned(),
            torrent: b"d8:announce0:4:infod4:name7:holidayee".to_vec(),
        }
    }

    #[test]
    fn a_capsule_survives_the_round_trip() {
        let key = crate::nexus::crypto::generate_content_key();
        let sealed = capsule().seal(&key, b"share", 1).expect("seals");

        assert_eq!(
            Capsule::open(&key, b"share", 1, &sealed).expect("opens"),
            capsule()
        );
    }

    #[test]
    fn a_capsule_is_unreadable_without_its_key() {
        let sealed = capsule()
            .seal(&crate::nexus::crypto::generate_content_key(), b"share", 1)
            .expect("seals");

        assert_eq!(
            Capsule::open(
                &crate::nexus::crypto::generate_content_key(),
                b"share",
                1,
                &sealed
            ),
            Err(CapsuleError::NotForThisKey)
        );
        assert!(
            !sealed.windows(7).any(|window| window == b"holiday"),
            "the plaintext is in the capsule the service is handed"
        );
    }

    #[test]
    fn a_capsule_cannot_be_moved_to_another_revision_or_collection() {
        let key = crate::nexus::crypto::generate_content_key();
        let sealed = capsule().seal(&key, b"share", 1).expect("seals");

        assert_eq!(
            Capsule::open(&key, b"share", 2, &sealed),
            Err(CapsuleError::NotForThisKey),
            "a capsule belongs to the revision it was sealed at"
        );
        assert_eq!(
            Capsule::open(&key, b"share2", 1, &sealed),
            Err(CapsuleError::NotForThisKey),
            "a capsule belongs to the share it was sealed for"
        );
    }

    #[test]
    fn sealing_twice_never_produces_the_same_bytes() {
        let key = crate::nexus::crypto::generate_content_key();

        let once = capsule().seal(&key, b"share", 1).expect("seals");
        let twice = capsule().seal(&key, b"share", 1).expect("seals");

        assert_ne!(once, twice, "nonce must differ");
    }

    #[test]
    fn nonsense_is_refused_rather_than_allocated() {
        let key = crate::nexus::crypto::generate_content_key();

        assert_eq!(
            Capsule::open(&key, b"share", 1, &[]),
            Err(CapsuleError::Malformed)
        );
    }

    #[test]
    fn a_capsule_is_refused_if_it_exceeds_the_size_limit() {
        let mut oversized = Capsule {
            name: "x".repeat(MAX_NAME_BYTES + 1),
            torrent: vec![0; 100],
        };

        assert_eq!(
            oversized.seal(&crate::nexus::crypto::generate_content_key(), b"share", 1),
            Err(CapsuleError::NameTooLong)
        );

        oversized = Capsule {
            name: "valid".to_owned(),
            torrent: vec![0; MAX_CAPSULE_BYTES + 1],
        };

        assert_eq!(
            oversized.seal(&crate::nexus::crypto::generate_content_key(), b"share", 1),
            Err(CapsuleError::TooLarge)
        );
    }
}
