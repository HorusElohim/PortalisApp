//! Tuning for one supervised client connection.

use std::time::Duration;

use crate::reconnect::ReconnectPolicy;

/// How long a command waits for its correlated response before failing.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How long one attempt to reach a service may take before it is abandoned.
///
/// Separate from [`DEFAULT_REQUEST_TIMEOUT`] because the two answer different
/// questions, and QUIC is what made the difference matter. A WebSocket to a
/// port with nothing behind it is refused at once, so bounding the handshake
/// with the request timeout cost nothing. A QUIC dial to a node that is not
/// there is not refused by anybody — there is no listener to say no — so the
/// bound is the only thing that ends it, and it is reached on every attempt.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    /// Governs the first connection attempt and every later reconnect.
    pub reconnect: ReconnectPolicy,
    /// How long one command waits for its correlated response.
    pub request_timeout: Duration,
    /// How long one attempt to reach the service may take.
    ///
    /// Bounds the whole handshake, so a peer that accepts a connection but
    /// never greets cannot stall a caller or the supervisor — and so a service
    /// that is simply not there costs one of these rather than forever.
    pub handshake_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            reconnect: ReconnectPolicy::default(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_protocol_request_timeout() {
        let config = ClientConfig::default();

        assert_eq!(config.request_timeout, DEFAULT_REQUEST_TIMEOUT);
        assert_eq!(config.handshake_timeout, DEFAULT_HANDSHAKE_TIMEOUT);
        assert_eq!(config.reconnect, ReconnectPolicy::default());
    }
}
