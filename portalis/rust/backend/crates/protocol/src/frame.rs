//! Bounded encoding and decoding of one protocol frame.
//!
//! A WebSocket delimits messages for us; a QUIC stream does not, so a frame
//! sent over one carries its length in front. Both ends of that agreement live
//! here rather than in the two transports, because a length prefix is the kind
//! of thing that is easy to write twice and easy to write twice differently.

use prost::Message;
use thiserror::Error;

use crate::limits::MAX_FRAME_BYTES;
use crate::v1;
use crate::validate::ValidationError;

/// A frame on a byte stream is a four-byte big-endian length, then that many
/// bytes.
pub const LENGTH_PREFIX_BYTES: usize = 4;

/// The prefix announcing `frame`.
///
/// Takes the frame rather than a length, so there is no way to announce a
/// number that is not the one about to be written.
#[must_use]
pub fn length_prefix(frame: &[u8]) -> [u8; LENGTH_PREFIX_BYTES] {
    // Bounded by `MAX_FRAME_BYTES`, which is far below `u32::MAX`, and every
    // frame reaching a transport has been through `encode_frame`.
    let length = u32::try_from(frame.len()).unwrap_or(u32::MAX);
    length.to_be_bytes()
}

/// How many bytes follow, or [`FrameError::TooLarge`] if the peer claims more
/// than the limit.
///
/// Checked before the reader allocates, which is the entire point of having a
/// limit: a peer that asks for a gigabyte should be refused, not accommodated
/// and then complained about.
///
/// # Errors
///
/// Returns [`FrameError::TooLarge`] when the announced length is over
/// [`MAX_FRAME_BYTES`].
pub fn frame_length(prefix: [u8; LENGTH_PREFIX_BYTES]) -> Result<usize, FrameError> {
    let actual = u32::from_be_bytes(prefix) as usize;
    validate_frame_size(actual)?;
    Ok(actual)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("frame exceeds the {MAX_FRAME_BYTES}-byte limit: {actual} bytes")]
    TooLarge { actual: usize },
    #[error("frame is not a valid protobuf envelope")]
    Malformed,
    #[error(transparent)]
    InvalidEnvelope(#[from] ValidationError),
}

/// Encodes one bounded protobuf WebSocket binary message.
///
/// # Errors
///
/// Returns [`FrameError`] when the envelope is invalid or encoded data exceeds
/// the protocol frame limit.
pub fn encode_frame(envelope: &v1::Envelope) -> Result<Vec<u8>, FrameError> {
    envelope.validate()?;
    let bytes = envelope.encode_to_vec();
    validate_frame_size(bytes.len())?;
    Ok(bytes)
}

/// Decodes and validates one bounded protobuf WebSocket binary message.
///
/// # Errors
///
/// Returns [`FrameError`] when the frame is too large, malformed, or violates
/// the envelope invariants.
pub fn decode_frame(bytes: &[u8]) -> Result<v1::Envelope, FrameError> {
    validate_frame_size(bytes.len())?;
    let envelope = v1::Envelope::decode(bytes).map_err(|_| FrameError::Malformed)?;
    envelope.validate()?;
    Ok(envelope)
}

/// Validates a WebSocket binary payload length before allocating or decoding.
///
/// # Errors
///
/// Returns [`FrameError::TooLarge`] when the payload exceeds the protocol
/// limit.
pub fn validate_frame_size(actual: usize) -> Result<(), FrameError> {
    if actual > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge { actual });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::new_message_id;
    use crate::v1::envelope::Payload;
    use crate::v1::{Envelope, Ping, ProtocolError, ProtocolErrorCode};

    fn ping_envelope() -> Envelope {
        Envelope {
            message_id: new_message_id(),
            correlation_id: Vec::new(),
            timestamp_unix_ns: 1,
            payload: Some(Payload::Ping(Ping { nonce: 7 })),
        }
    }

    #[test]
    fn rejects_oversized_frame_without_allocating() {
        assert!(matches!(
            validate_frame_size(MAX_FRAME_BYTES + 1),
            Err(FrameError::TooLarge { actual }) if actual == MAX_FRAME_BYTES + 1
        ));
        assert!(validate_frame_size(MAX_FRAME_BYTES).is_ok());
    }

    #[test]
    fn rejects_malformed_or_invalid_frames() {
        assert_eq!(decode_frame(&[0xff]), Err(FrameError::Malformed));
        assert_eq!(
            decode_frame(&[]),
            Err(FrameError::InvalidEnvelope(
                ValidationError::InvalidMessageId { actual: 0 }
            ))
        );
    }

    #[test]
    fn encodes_and_decodes_valid_frames() {
        let envelope = ping_envelope();

        assert_eq!(
            decode_frame(&encode_frame(&envelope).expect("valid frame")).expect("valid frame"),
            envelope
        );
    }

    #[test]
    fn rejects_invalid_or_oversized_encoded_frames() {
        let mut invalid = ping_envelope();
        invalid.message_id.clear();
        assert_eq!(
            encode_frame(&invalid),
            Err(FrameError::InvalidEnvelope(
                ValidationError::InvalidMessageId { actual: 0 }
            ))
        );

        let oversized = Envelope {
            message_id: new_message_id(),
            correlation_id: Vec::new(),
            timestamp_unix_ns: 1,
            payload: Some(Payload::ProtocolError(ProtocolError {
                code: ProtocolErrorCode::InvalidMessage as i32,
                message: "x".repeat(MAX_FRAME_BYTES),
                retry_after_ms: None,
                retryable: false,
            })),
        };
        let encoded_length = oversized.encode_to_vec().len();
        assert_eq!(
            encode_frame(&oversized),
            Err(FrameError::TooLarge {
                actual: encoded_length,
            })
        );
        assert_eq!(
            decode_frame(&vec![0; MAX_FRAME_BYTES + 1]),
            Err(FrameError::TooLarge {
                actual: MAX_FRAME_BYTES + 1,
            })
        );
    }

    /// A prefix says exactly what follows, and a claim over the limit is
    /// refused on the prefix alone — before a reader has allocated for it.
    #[test]
    fn a_frame_announces_its_own_length_and_an_overlong_claim_is_refused() {
        let frame = encode_frame(&ping_envelope()).expect("encodes");
        assert_eq!(
            frame_length(length_prefix(&frame)),
            Ok(frame.len()),
            "what is announced is what is written"
        );
        assert_eq!(frame_length(length_prefix(b"")), Ok(0));

        let too_long = u32::try_from(MAX_FRAME_BYTES + 1).expect("bounded");
        assert_eq!(
            frame_length(too_long.to_be_bytes()),
            Err(FrameError::TooLarge {
                actual: MAX_FRAME_BYTES + 1
            })
        );
        // The largest claim there is, refused the same way.
        assert!(matches!(
            frame_length(u32::MAX.to_be_bytes()),
            Err(FrameError::TooLarge { .. })
        ));
    }
}
