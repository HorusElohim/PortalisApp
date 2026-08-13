//! Talking to a peer directly, with no service in the path.
//!
//! This is the product claim, and `SPEC.md` §14: two devices on one network
//! share with each other whether or not a service exists. The service is a
//! convenience for reaching devices that are not on your network — it is not
//! what makes sharing work, and the way to keep that true is for the peer path
//! to be the one that is written first and tested without a service running.
//!
//! A session carries objects, and nothing else. It does not verify them: an
//! object is valid or invalid on its own terms, and where it came from changes
//! nothing (§9). So this module can be wrong about who it is talking to
//! without being able to make a caller believe a forged revision — the worst a
//! hostile peer achieves is wasting a round trip.
//!
//! What a session *does* decide is [`Security`], and it reports it the moment
//! the handshake completes rather than inferring it later (§15). Two facts:
//! whether the bytes travel directly or through a relay, and whether the key on
//! the other end belongs to someone whose fingerprint has been compared. Both
//! are outputs of the connection, so neither can drift from what is actually
//! happening.

use std::collections::HashSet;

use portalis_nexus_protocol::{DEVICE_KEY_BYTES, MAX_FRAME_BYTES};
use thiserror::Error;

use crate::endpoint::{ConnectionPath, NEXUS_ALPN, NexusEndpoint};
use iroh::PublicKey;
use iroh::endpoint::Connection;

/// The most a peer may ask for or send in one exchange.
///
/// A publication is a manifest and its entry payloads, so it is bounded by the
/// same limit as a frame. A peer that claims more is refused before anything
/// is allocated for it.
pub const MAX_OBJECT_BYTES: usize = MAX_FRAME_BYTES;

/// Whether the bytes are travelling directly or through a relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Direct,
    Relayed,
}

/// How much is known about the key on the other end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerTrust {
    /// A contact whose fingerprint has been compared (D4).
    Known,
    /// A known contact whose fingerprint has not been compared yet.
    Unverified,
    /// Authenticated, and belonging to nobody we know.
    Unknown,
}

/// What this connection actually is, as of the handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Security {
    pub path: Path,
    pub peer: PeerTrust,
}

/// Who this device is willing to talk to, and how well it knows them.
///
/// Held by the caller because it comes from the contact store, which this
/// crate does not have. Passing it in also makes the refusal testable without
/// a database.
#[derive(Clone, Debug, Default)]
pub struct KnownPeers {
    /// Contacts whose fingerprint has been compared.
    verified: HashSet<[u8; DEVICE_KEY_BYTES]>,
    /// Contacts we know of, whose fingerprint has not been compared.
    unverified: HashSet<[u8; DEVICE_KEY_BYTES]>,
}

impl KnownPeers {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A contact whose fingerprint has been compared.
    #[must_use]
    pub fn verified(mut self, key: [u8; DEVICE_KEY_BYTES]) -> Self {
        self.unverified.remove(&key);
        self.verified.insert(key);
        self
    }

    /// A contact we know of but have not compared fingerprints with.
    #[must_use]
    pub fn unverified(mut self, key: [u8; DEVICE_KEY_BYTES]) -> Self {
        if !self.verified.contains(&key) {
            self.unverified.insert(key);
        }
        self
    }

    fn trust(&self, key: &[u8; DEVICE_KEY_BYTES]) -> PeerTrust {
        if self.verified.contains(key) {
            PeerTrust::Known
        } else if self.unverified.contains(key) {
            PeerTrust::Unverified
        } else {
            PeerTrust::Unknown
        }
    }
}

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

impl SessionError {
    /// Turns any transport failure into one this crate's callers understand.
    ///
    /// A named function rather than a closure at each call site, so
    /// `map_err(SessionError::connection)` reads as what it is and there is
    /// one conversion to test rather than one per operation.
    fn connection(error: impl std::fmt::Display) -> Self {
        Self::Connection(error.to_string())
    }
}

/// What one peer asks another for.
///
/// Deliberately tiny. A session moves objects that verify on their own terms,
/// so there is nothing here that needs negotiating, and every request a peer
/// can express is one this device would happily answer for anybody it talks
/// to at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// The current publication of a collection, whatever revision that is.
    Publication { collection_id: [u8; 16] },
}

impl Request {
    const PUBLICATION: u8 = 1;

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Publication { collection_id } => {
                let mut bytes = Vec::with_capacity(17);
                bytes.push(Self::PUBLICATION);
                bytes.extend_from_slice(collection_id);
                bytes
            }
        }
    }

    /// # Errors
    ///
    /// Returns [`SessionError::Unintelligible`] for anything this device does
    /// not recognise, including a future request kind.
    pub fn decode(bytes: &[u8]) -> Result<Self, SessionError> {
        match bytes.split_first() {
            Some((&Self::PUBLICATION, rest)) if rest.len() == 16 => {
                let mut collection_id = [0_u8; 16];
                collection_id.copy_from_slice(rest);
                Ok(Self::Publication { collection_id })
            }
            _ => Err(SessionError::Unintelligible),
        }
    }
}

/// One connection to one peer.
#[derive(Debug)]
pub struct Session {
    connection: Connection,
    security: Security,
    remote: [u8; DEVICE_KEY_BYTES],
}

impl Session {
    /// Dials `peer` and reports what the resulting connection is.
    ///
    /// An address rather than an identity, because reaching a device needs
    /// somewhere to send packets. On a local network that comes from
    /// discovery; off it, from the rendezvous service — which is the one thing
    /// a service is actually needed for, and is not needed here.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::UnknownPeer`] when the remote key is not a
    /// contact, or [`SessionError::Connection`] when the connection fails.
    pub async fn connect(
        endpoint: &NexusEndpoint,
        peer: impl Into<iroh::NodeAddr>,
        known: &KnownPeers,
    ) -> Result<Self, SessionError> {
        let connection = endpoint
            .connect(peer, NEXUS_ALPN)
            .await
            .map_err(SessionError::connection)?;
        Self::establish(endpoint, connection, known)
    }

    /// Accepts a connection a peer has already opened.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] for the same reasons [`Self::connect`] does.
    pub fn accept(
        endpoint: &NexusEndpoint,
        connection: Connection,
        known: &KnownPeers,
    ) -> Result<Self, SessionError> {
        Self::establish(endpoint, connection, known)
    }

    /// The one place a connection becomes a session, so the refusal and the
    /// security report cannot be applied to one direction and not the other.
    fn establish(
        endpoint: &NexusEndpoint,
        connection: Connection,
        known: &KnownPeers,
    ) -> Result<Self, SessionError> {
        let remote_id = connection
            .remote_node_id()
            .map_err(SessionError::connection)?;
        let remote = *PublicKey::as_bytes(&remote_id);

        let peer = known.trust(&remote);
        if peer == PeerTrust::Unknown {
            // Refused here rather than at the first request: an unknown peer
            // should not learn what collections exist by asking.
            return Err(SessionError::UnknownPeer);
        }

        Ok(Self {
            connection,
            security: Security {
                path: match endpoint.path_to(remote_id) {
                    ConnectionPath::Direct => Path::Direct,
                    // Mixed means some of it is relayed, and reporting the
                    // better half would overstate what is true.
                    _ => Path::Relayed,
                },
                peer,
            },
            remote,
        })
    }

    /// What this connection is, decided at the handshake and not revisited.
    #[must_use]
    pub const fn security(&self) -> Security {
        self.security
    }

    /// The peer's signing key.
    #[must_use]
    pub const fn remote(&self) -> [u8; DEVICE_KEY_BYTES] {
        self.remote
    }

    /// Asks the peer for something and reads the answer.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the stream fails or the peer answers with
    /// more than the limit allows.
    pub async fn request(&self, request: Request) -> Result<Vec<u8>, SessionError> {
        let (mut send, mut receive) = self
            .connection
            .open_bi()
            .await
            .map_err(SessionError::connection)?;

        send.write_all(&request.encode())
            .await
            .map_err(SessionError::connection)?;
        send.finish().map_err(SessionError::connection)?;

        receive
            .read_to_end(MAX_OBJECT_BYTES)
            .await
            .map_err(SessionError::connection)
    }

    /// Reads one request from the peer and hands back the stream to answer on.
    ///
    /// Returning the responder rather than taking a closure keeps the decision
    /// about *what* to answer with the caller, which is the only part that
    /// knows about collections.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the peer closes early or sends something
    /// unintelligible.
    pub async fn next_request(&self) -> Result<(Request, Responder), SessionError> {
        let (send, mut receive) = self
            .connection
            .accept_bi()
            .await
            .map_err(SessionError::connection)?;

        let asked = receive
            .read_to_end(MAX_OBJECT_BYTES)
            .await
            .map_err(|_| SessionError::Incomplete)?;
        Ok((Request::decode(&asked)?, Responder { send }))
    }

    /// Closes the connection.
    pub fn close(&self) {
        self.connection.close(0_u32.into(), b"done");
    }
}

/// The open half of a stream, waiting for an answer.
#[derive(Debug)]
pub struct Responder {
    send: iroh::endpoint::SendStream,
}

impl Responder {
    /// Sends `object` and finishes the stream.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::TooLarge`] for an object over the limit, or
    /// [`SessionError::Connection`] when the write fails.
    pub async fn answer(mut self, object: &[u8]) -> Result<(), SessionError> {
        if object.len() > MAX_OBJECT_BYTES {
            return Err(SessionError::TooLarge {
                actual: object.len(),
            });
        }
        self.send
            .write_all(object)
            .await
            .map_err(SessionError::connection)?;
        self.send.finish().map_err(SessionError::connection)
    }
}

#[cfg(test)]
mod tests {
    //! The connection half needs two real endpoints, so these are integration
    //! tests in all but name. What can be checked without one — the request
    //! encoding and who counts as known — is checked without one.

    use super::*;

    const ADA: [u8; DEVICE_KEY_BYTES] = [1; DEVICE_KEY_BYTES];
    const MIRA: [u8; DEVICE_KEY_BYTES] = [2; DEVICE_KEY_BYTES];
    const STRANGER: [u8; DEVICE_KEY_BYTES] = [9; DEVICE_KEY_BYTES];

    #[test]
    fn a_request_round_trips_and_anything_else_is_refused() {
        let request = Request::Publication {
            collection_id: [7; 16],
        };

        let encoded = request.encode();
        assert_eq!(Request::decode(&encoded).expect("decodes"), request);

        // A kind from a future version, a truncated one, and nothing at all.
        for refused in [vec![9, 0], encoded[..8].to_vec(), Vec::new()] {
            assert!(matches!(
                Request::decode(&refused),
                Err(SessionError::Unintelligible)
            ));
        }
        // Right kind, wrong length.
        let mut padded = encoded;
        padded.push(0);
        assert!(matches!(
            Request::decode(&padded),
            Err(SessionError::Unintelligible)
        ));
    }

    #[test]
    fn a_peer_is_known_only_as_well_as_it_has_been_verified() {
        let known = KnownPeers::new().verified(ADA).unverified(MIRA);

        assert_eq!(known.trust(&ADA), PeerTrust::Known);
        assert_eq!(known.trust(&MIRA), PeerTrust::Unverified);
        assert_eq!(known.trust(&STRANGER), PeerTrust::Unknown);
        assert_eq!(KnownPeers::default().trust(&ADA), PeerTrust::Unknown);
    }

    /// Comparing a fingerprint is a one-way step: it cannot be undone by
    /// re-adding the same contact, which would otherwise quietly downgrade
    /// what the interface says about them.
    #[test]
    fn verifying_a_contact_outranks_knowing_of_them_in_either_order() {
        assert_eq!(
            KnownPeers::new().verified(ADA).unverified(ADA).trust(&ADA),
            PeerTrust::Known
        );
        assert_eq!(
            KnownPeers::new().unverified(ADA).verified(ADA).trust(&ADA),
            PeerTrust::Known
        );
    }

    #[test]
    fn a_refusal_says_which_one_it_is() {
        assert!(
            SessionError::UnknownPeer
                .to_string()
                .contains("not a contact")
        );
        assert!(
            SessionError::TooLarge { actual: 99 }
                .to_string()
                .contains("99")
        );
        assert!(
            SessionError::Incomplete
                .to_string()
                .contains("closed the connection")
        );
        assert!(
            SessionError::Connection("refused".to_owned())
                .to_string()
                .contains("refused")
        );
    }

    /// Two real endpoints on the loopback: a known peer gets an answer, an
    /// unknown one never becomes a session at all.
    use crate::endpoint::NEXUS_ALPN;
    use ed25519_dalek::SigningKey;

    async fn bound(seed: u8) -> NexusEndpoint {
        NexusEndpoint::bind(
            SigningKey::from_bytes(&[seed; 32]).to_bytes(),
            vec![NEXUS_ALPN.to_vec()],
            iroh::RelayMode::Disabled,
        )
        .await
        .expect("binds")
    }

    fn key_of(endpoint: &NexusEndpoint) -> [u8; DEVICE_KEY_BYTES] {
        *PublicKey::as_bytes(&endpoint.id())
    }

    /// Every way a live exchange can fail, forced rather than waited for.
    ///
    /// These are one-line mappings, but a mapping nothing exercises is a
    /// mapping that can name the wrong failure. Each case below fails
    /// immediately: none of them waits for a dial to time out, because a test
    /// that costs thirty seconds is a test that gets skipped.
    #[tokio::test]
    async fn every_way_a_connection_fails_is_reported() {
        let client = bound(31).await;
        let server = bound(32).await;
        let known = KnownPeers::new().verified(key_of(&server));

        // Nowhere to send packets: an address with no addresses in it, and no
        // relay, fails at once rather than retrying. Built as an `EndpointAddr`
        // rather than an identity so this exercises the same instantiation of
        // `connect` the successful call below does.
        assert!(matches!(
            Session::connect(&client, iroh::NodeAddr::new(server.id()), &known).await,
            Err(SessionError::Connection(_))
        ));

        // A peer that reads the request and then vanishes: the answer never
        // arrives, and the read reports rather than hanging.
        let address = server.addr_when_ready().await;
        let will_answer = KnownPeers::new().verified(key_of(&client));
        let vanishing = tokio::spawn(async move {
            let Some(incoming) = server.accept().await else {
                return;
            };
            let Ok(connection) = incoming.await else {
                return;
            };
            let session = Session::accept(&server, connection, &will_answer).expect("known");
            let (_, responder) = session.next_request().await.expect("a request");
            // Closed underneath the responder, so answering fails on write.
            session.close();
            assert!(matches!(
                responder.answer(b"too late").await,
                Err(SessionError::Connection(_))
            ));
        });

        let session = Session::connect(&client, address, &known)
            .await
            .expect("connects");
        assert!(matches!(
            session
                .request(Request::Publication {
                    collection_id: [1; 16]
                })
                .await,
            Err(SessionError::Connection(_))
        ));
        vanishing.await.expect("the vanishing peer");
    }

    #[tokio::test]
    async fn a_known_peer_is_served_and_an_unknown_one_is_refused() {
        let server = bound(11).await;
        let client = bound(12).await;
        let stranger = bound(13).await;

        let address = server.addr_when_ready().await;
        let server_key = key_of(&server);
        let answers_to = KnownPeers::new().verified(key_of(&client));
        let (oversized_tx, mut oversized_rx) = tokio::sync::mpsc::channel(1);
        // Runs until aborted. Ending it after one answer would drop the
        // connection while the client is still reading from it.
        let listening = tokio::spawn(async move {
            while let Some(incoming) = server.accept().await {
                let Ok(connection) = incoming.await else {
                    continue;
                };
                let Ok(session) = Session::accept(&server, connection, &answers_to) else {
                    // Unknown: never becomes a session, so never answers.
                    continue;
                };
                let mut answered = 0_u32;
                while let Ok((request, responder)) = session.next_request().await {
                    assert_eq!(
                        request,
                        Request::Publication {
                            collection_id: [3; 16]
                        }
                    );
                    answered += 1;
                    if answered == 1 {
                        let _ = responder.answer(b"the publication").await;
                    } else {
                        // Refused before a byte of it is written, rather than
                        // transmitted and then complained about.
                        let oversized = vec![0_u8; MAX_OBJECT_BYTES + 1];
                        let refused = responder.answer(&oversized).await;
                        let _ = oversized_tx.try_send(matches!(
                            refused,
                            Err(SessionError::TooLarge { actual })
                                if actual == MAX_OBJECT_BYTES + 1
                        ));
                    }
                }
            }
        });

        let knows_the_server = KnownPeers::new().verified(server_key);
        let session = Session::connect(&client, address.clone(), &knows_the_server)
            .await
            .expect("a known peer connects");
        assert_eq!(session.security().peer, PeerTrust::Known);
        assert_eq!(session.security().path, Path::Direct);
        assert_eq!(session.remote(), server_key);
        assert_eq!(
            session
                .request(Request::Publication {
                    collection_id: [3; 16]
                })
                .await
                .expect("an answer"),
            b"the publication"
        );
        // A second ask, which the server tries to over-answer.
        let _ = session
            .request(Request::Publication {
                collection_id: [3; 16],
            })
            .await;
        assert!(
            oversized_rx.recv().await.expect("the server reported"),
            "an object over the limit is refused rather than sent"
        );
        // Used after closing: every stream operation reports rather than
        // panicking or hanging.
        session.close();
        assert!(matches!(
            session
                .request(Request::Publication {
                    collection_id: [3; 16]
                })
                .await,
            Err(SessionError::Connection(_))
        ));
        assert!(matches!(
            session.next_request().await,
            Err(SessionError::Connection(_))
        ));

        // The stranger does not know the server either, so its own side
        // refuses before a packet of application data is sent.
        assert!(matches!(
            Session::connect(&stranger, address, &KnownPeers::new()).await,
            Err(SessionError::UnknownPeer | SessionError::Connection(_))
        ));
        listening.abort();
    }
}
