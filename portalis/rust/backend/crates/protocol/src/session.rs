//! The request vocabulary shared by peers and services.
//!
//! Connections belong to the client, and persistence belongs to storage. The
//! bytes they exchange belong here so neither implementation has to depend on
//! the other merely to agree on the wire.

use thiserror::Error;

use crate::{DEVICE_ID_BYTES, DEVICE_KEY_BYTES, MAX_FRAME_BYTES};

/// The most a peer may ask for or send in one exchange.
///
/// A publication is a manifest and its entry payloads, so it is bounded by the
/// same limit as a frame. A peer that claims more is refused before anything
/// is allocated for it.
pub const MAX_OBJECT_BYTES: usize = MAX_FRAME_BYTES;

/// Why an exchange did not happen.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The remote key belongs to nobody this device knows. Refused before any
    /// object is asked for or sent, so an unknown peer learns nothing beyond
    /// the fact that something is listening.
    #[error("that peer is not a contact of this device")]
    UnknownPeer,
    #[error("the peer asked for or sent {actual} bytes, over the {MAX_OBJECT_BYTES}-byte limit")]
    TooLarge { actual: usize },
    #[error("the peer closed the connection before finishing")]
    Incomplete,
    #[error("the peer sent a request this device does not understand")]
    Unintelligible,
    #[error("the connection failed: {0}")]
    Connection(String),
}

/// What one party asks another for.
///
/// One vocabulary for peers and for the service, deliberately. A service is a
/// peer that also stores: it answers the same requests, and a client that
/// fetches a device log does not care whether the bytes came from the person
/// who signed it or from something holding a copy. That is only safe because
/// an object is valid on its own terms — if where it came from mattered, this
/// enum would have to be two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The current publication of a collection, whatever revision that is.
    Publication { collection_id: [u8; 16] },
    /// Everything waiting for this device. Answered only by a service.
    Collect,
    /// Leave something for a device that is not reachable.
    Deliver {
        device: [u8; DEVICE_ID_BYTES],
        body: Vec<u8>,
    },
    /// Somebody's device log, so it can be verified against what is held.
    DeviceLog { root_key: [u8; DEVICE_KEY_BYTES] },
}

impl Request {
    const PUBLICATION: u8 = 1;
    const COLLECT: u8 = 2;
    const DELIVER: u8 = 3;
    const DEVICE_LOG: u8 = 4;

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        match self {
            Self::Publication { collection_id } => {
                bytes.push(Self::PUBLICATION);
                bytes.extend_from_slice(collection_id);
            }
            Self::Collect => bytes.push(Self::COLLECT),
            Self::Deliver { device, body } => {
                bytes.push(Self::DELIVER);
                bytes.extend_from_slice(device);
                bytes.extend_from_slice(body);
            }
            Self::DeviceLog { root_key } => {
                bytes.push(Self::DEVICE_LOG);
                bytes.extend_from_slice(root_key);
            }
        }
        bytes
    }

    /// # Errors
    ///
    /// Returns [`SessionError::Unintelligible`] for anything this device does
    /// not recognise, including a request kind from a newer version.
    pub fn decode(bytes: &[u8]) -> Result<Self, SessionError> {
        let (&kind, rest) = bytes.split_first().ok_or(SessionError::Unintelligible)?;
        match (kind, rest.len()) {
            (Self::PUBLICATION, 16) => Ok(Self::Publication {
                collection_id: fixed(rest)?,
            }),
            (Self::COLLECT, 0) => Ok(Self::Collect),
            (Self::DELIVER, length) if length > DEVICE_ID_BYTES => {
                let (device, body) = rest.split_at(DEVICE_ID_BYTES);
                Ok(Self::Deliver {
                    device: fixed(device)?,
                    body: body.to_vec(),
                })
            }
            (Self::DEVICE_LOG, DEVICE_KEY_BYTES) => Ok(Self::DeviceLog {
                root_key: fixed(rest)?,
            }),
            _ => Err(SessionError::Unintelligible),
        }
    }
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], SessionError> {
    <[u8; N]>::try_from(bytes).map_err(|_| SessionError::Unintelligible)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_round_trips_and_anything_else_is_refused() {
        let requests = [
            Request::Publication {
                collection_id: [7; 16],
            },
            Request::Collect,
            Request::Deliver {
                device: [8; DEVICE_ID_BYTES],
                body: b"a publication".to_vec(),
            },
            Request::DeviceLog {
                root_key: [9; DEVICE_KEY_BYTES],
            },
        ];

        for request in &requests {
            let encoded = request.encode();
            assert_eq!(&Request::decode(&encoded).expect("decodes"), request);

            // A fixed-width request refuses a byte more or a byte less.
            // `Deliver` is exempt from both: its body is the part that varies,
            // so a longer or shorter one is simply a different valid request.
            if !matches!(request, Request::Deliver { .. }) {
                let mut padded = encoded.clone();
                padded.push(0);
                assert!(
                    matches!(Request::decode(&padded), Err(SessionError::Unintelligible)),
                    "{request:?} accepted a trailing byte"
                );
                let truncated = Request::decode(&encoded[..encoded.len() - 1]);
                assert!(
                    matches!(truncated, Err(SessionError::Unintelligible)),
                    "{request:?} accepted a truncation"
                );
            }
        }

        // A kind from a future version, and nothing at all.
        for refused in [vec![9_u8, 0], Vec::new()] {
            assert!(matches!(
                Request::decode(&refused),
                Err(SessionError::Unintelligible)
            ));
        }
        // A delivery with an address and no body is not a delivery.
        assert!(matches!(
            Request::decode(
                &Request::Deliver {
                    device: [8; DEVICE_ID_BYTES],
                    body: Vec::new(),
                }
                .encode()
            ),
            Err(SessionError::Unintelligible)
        ));
    }
}
