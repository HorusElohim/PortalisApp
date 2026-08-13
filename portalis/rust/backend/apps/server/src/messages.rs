//! Envelope construction and inbound message dispatch.
//!
//! Every transport calls the same dispatch decisions from here.

use portalis_nexus_protocol::v1::envelope::Payload;
use portalis_nexus_protocol::v1::{
    Authenticated, Envelope, Ping, Pong, PresenceEvent, ProtocolError, ProtocolErrorCode,
    ServerHello,
};
use portalis_nexus_protocol::{
    CURRENT_PROTOCOL_VERSION, encode_frame, new_challenge, new_message_id,
};
use portalis_nexus_server_core::{Identity, ProtocolPolicy, UserId};

#[must_use]
pub fn server_hello(protocol_policy: &ProtocolPolicy, server_time_unix_ns: u64) -> Envelope {
    hello_envelope(
        hello_payload(protocol_policy, server_time_unix_ns),
        server_time_unix_ns,
    )
}

/// Wraps an already-built hello, so a caller can read its connection ID first.
#[must_use]
pub fn hello_envelope(hello: ServerHello, timestamp_unix_ns: u64) -> Envelope {
    Envelope {
        message_id: new_message_id(),
        correlation_id: Vec::new(),
        timestamp_unix_ns,
        payload: Some(Payload::ServerHello(hello)),
    }
}

#[must_use]
pub fn hello_payload(protocol_policy: &ProtocolPolicy, server_time_unix_ns: u64) -> ServerHello {
    ServerHello {
        connection_id: new_message_id(),
        challenge: new_challenge(),
        server_time_unix_ns,
        supported_protocols: Some(*protocol_policy.supported()),
    }
}

#[must_use]
pub fn response_for(envelope: &Envelope, timestamp_unix_ns: u64) -> Envelope {
    match &envelope.payload {
        Some(Payload::Ping(Ping { nonce })) => Envelope {
            message_id: new_message_id(),
            correlation_id: envelope.message_id.clone(),
            timestamp_unix_ns,
            payload: Some(Payload::Pong(Pong { nonce: *nonce })),
        },
        _ => protocol_error(
            ProtocolErrorCode::InvalidMessage,
            envelope.message_id.clone(),
            "only Ping is accepted before authentication".to_owned(),
            timestamp_unix_ns,
        ),
    }
}

#[must_use]
pub fn protocol_error(
    code: ProtocolErrorCode,
    correlation_id: Vec<u8>,
    message: String,
    timestamp_unix_ns: u64,
) -> Envelope {
    Envelope {
        message_id: new_message_id(),
        correlation_id,
        timestamp_unix_ns,
        payload: Some(Payload::ProtocolError(ProtocolError {
            code: code as i32,
            message,
            retry_after_ms: None,
            retryable: false,
        })),
    }
}

/// Answers `request` with `payload`, correlated to it.
#[must_use]
pub fn reply_with(request: &Envelope, payload: Payload, timestamp_unix_ns: u64) -> Envelope {
    Envelope {
        message_id: new_message_id(),
        correlation_id: request.message_id.clone(),
        timestamp_unix_ns,
        payload: Some(payload),
    }
}

/// Confirms which identity a connection is now bound to.
#[must_use]
pub fn authenticated_reply(
    request: &Envelope,
    identity: &Identity,
    timestamp_unix_ns: u64,
) -> Envelope {
    Envelope {
        message_id: new_message_id(),
        correlation_id: request.message_id.clone(),
        timestamp_unix_ns,
        payload: Some(Payload::Authenticated(Authenticated {
            user_id: identity.user.user_id.to_vec(),
            device_id: identity.device.device_id.to_vec(),
            username: identity.user.username.clone(),
            discriminator: identity.user.discriminator.clone(),
            protocol_version: CURRENT_PROTOCOL_VERSION,
        })),
    }
}

/// Announces where a user stands, unsolicited: it answers no request.
#[must_use]
pub fn presence_event(
    user_id: UserId,
    online: bool,
    last_seen_unix_ns: Option<u64>,
    timestamp_unix_ns: u64,
) -> Envelope {
    Envelope {
        message_id: new_message_id(),
        correlation_id: Vec::new(),
        timestamp_unix_ns,
        payload: Some(Payload::PresenceEvent(PresenceEvent {
            user_id: user_id.to_vec(),
            online,
            last_seen_unix_ns,
        })),
    }
}

/// Encodes one server-generated envelope as a bounded binary frame.
///
/// Bytes, not a transport's message type: the same frame is written to the
/// QUIC stream and queued for a live connection.
///
/// # Panics
///
/// Panics when the envelope is invalid or exceeds the frame limit, which would
/// mean the server itself built a message the protocol forbids.
#[must_use]
pub fn binary_frame(envelope: &Envelope) -> Vec<u8> {
    encode_frame(envelope).expect("server-generated envelopes are valid and bounded")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ProtocolPolicy {
        ProtocolPolicy::new(CURRENT_PROTOCOL_VERSION, CURRENT_PROTOCOL_VERSION)
            .expect("valid protocol range")
    }

    fn rejection(message: &str) -> Payload {
        Payload::ProtocolError(ProtocolError {
            code: ProtocolErrorCode::InvalidMessage as i32,
            message: message.to_owned(),
            retry_after_ms: None,
            retryable: false,
        })
    }

    #[test]
    fn server_hello_is_valid_for_the_current_protocol() {
        let hello = hello_payload(&policy(), 42);

        assert_eq!(hello.server_time_unix_ns, 42);
        assert_eq!(
            portalis_nexus_protocol::validate_server_hello(&hello),
            Ok(())
        );
        assert_eq!(server_hello(&policy(), 42).timestamp_unix_ns, 42);
    }

    #[test]
    fn responds_to_ping_and_rejects_other_messages() {
        let request = Envelope {
            message_id: new_message_id(),
            correlation_id: Vec::new(),
            timestamp_unix_ns: 1,
            payload: Some(Payload::Ping(Ping { nonce: 7 })),
        };
        let response = response_for(&request, 2);
        assert_eq!(response.correlation_id, request.message_id);
        assert_eq!(response.timestamp_unix_ns, 2);
        assert_eq!(response.payload, Some(Payload::Pong(Pong { nonce: 7 })));

        let rejected = response_for(&response, 3);
        assert_eq!(rejected.correlation_id, response.message_id);
        assert_eq!(
            rejected.payload,
            Some(rejection("only Ping is accepted before authentication"))
        );
    }
}
