//! The in-person collection invitation carried by a QR code or deep link.
//!
//! A magnet URI names content and endpoints and nothing else, so a receiver
//! learns what it scanned only after the swarm answers — until then the import
//! screen has no name, no size, and no way to tell a fresh code from one still
//! on screen from yesterday. This envelope carries the few facts the receiving
//! interface needs to describe the invitation before any network round trip,
//! alongside the same info hash and peer hints the magnet already held.
//!
//! It is compact rather than textual because QR density decides how far away a
//! phone can scan: every byte here is one the camera has to resolve. Fields are
//! fixed-width and length-prefixed for the same reason the rest of this module
//! is hand-written — the bytes are the contract, not a library's rendering of
//! it.
//!
//! It is deliberately **not** encrypted. A QR held up to a camera is readable
//! by everyone who can see it, so encrypting it would protect nothing while
//! implying a confidentiality this format does not have. Compression is applied
//! only when it actually shrinks the payload, and never changes what is
//! disclosed. Anything that must stay secret belongs in the content key, which
//! is not carried here.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use thiserror::Error;

/// The scheme and path prefix an invitation link is rendered with.
///
/// A custom scheme is what lets a phone's camera offer Portalis directly; a
/// bare `magnet:` has no such registration on iOS.
pub const INVITATION_PREFIX: &str = "portalis://c/";

/// The envelope version, rejected rather than guessed at when unknown.
const VERSION: u8 = 1;

/// Set when the body after the header byte is deflate-compressed.
const FLAG_DEFLATE: u8 = 0b1000_0000;

const VERSION_MASK: u8 = 0b0111_1111;

/// Bounds chosen so a malformed or hostile code is refused before anything is
/// allocated for it, and so an honest one stays inside a scannable QR.
pub const MAX_PEERS: usize = 24;
pub const MAX_TEXT_BYTES: usize = 96;
pub const MAX_ENCODED_BYTES: usize = 2 * 1024;

const TAG_V4: u8 = 4;
const TAG_V6: u8 = 6;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InvitationError {
    #[error("not a Portalis invitation link")]
    NotAnInvitation,
    #[error("invitation is not valid base64url")]
    Malformed,
    #[error("invitation envelope version {actual} is not supported")]
    UnsupportedVersion { actual: u8 },
    #[error("invitation ended before its {field} field")]
    Truncated { field: &'static str },
    #[error("invitation has {actual} trailing bytes")]
    Trailing { actual: usize },
    #[error("invitation {field} is not valid UTF-8")]
    NotUtf8 { field: &'static str },
    #[error("invitation {field} is {actual} bytes, over the {limit}-byte limit")]
    TooLong {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    #[error("invitation names {actual} peers, over the {MAX_PEERS}-peer limit")]
    TooManyPeers { actual: usize },
    #[error("invitation peer address tag {actual} is not 4 or 6")]
    UnknownAddressTag { actual: u8 },
    #[error("invitation could not be decompressed")]
    Decompression,
}

/// What a receiver can know about a shared collection before contacting anyone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    /// `BitTorrent` v1 info hash: the durable identity of the content.
    pub info_hash: [u8; 20],
    /// The collection's name, for the import screen's title.
    pub name: String,
    /// The sharing device's name, so the receiver can tell whose code this is.
    pub owner: String,
    /// How many entries the collection holds, so the grid can lay out
    /// placeholders before any metadata arrives.
    pub entries: u32,
    /// Seconds since the Unix epoch at which this invitation was produced.
    ///
    /// Peer addresses go stale when a device changes network, so a receiver
    /// that cannot connect can say *why* rather than only that it failed.
    pub issued_at_secs: u32,
    /// Direct endpoints where the sharing device was reachable when the code
    /// was produced. Untrusted routing hints, never authorization.
    pub peers: Vec<SocketAddr>,
}

impl Invitation {
    /// Renders this invitation as the app-routable link a QR encodes.
    #[must_use]
    pub fn encode(&self) -> String {
        let body = self.body();
        // Compression is a size optimisation, not part of the contract: a
        // 20-byte hash and packed addresses are already close to incompressible,
        // so deflate can easily produce *more* bytes than it consumed. Take the
        // result only when it actually helps, and say which one it is in the
        // header so the reader never has to guess.
        let (flag, payload) = match deflate(&body) {
            Some(packed) if packed.len() < body.len() => (FLAG_DEFLATE, packed),
            _ => (0, body),
        };
        let mut bytes = Vec::with_capacity(payload.len() + 1);
        bytes.push(VERSION | flag);
        bytes.extend_from_slice(&payload);
        format!("{INVITATION_PREFIX}{}", base64url(&bytes))
    }

    /// Parses a scanned or pasted invitation link.
    ///
    /// Every field is bounded and the whole body must be consumed exactly, so
    /// a truncated, padded, or hostile code is refused rather than partially
    /// believed.
    ///
    /// # Errors
    ///
    /// Returns [`InvitationError`] when the link is not an invitation, is not
    /// valid base64url, carries an unsupported version, is truncated or has
    /// trailing bytes, exceeds a field bound, or fails to decompress.
    pub fn decode(link: &str) -> Result<Self, InvitationError> {
        let encoded = link
            .trim()
            .strip_prefix(INVITATION_PREFIX)
            .ok_or(InvitationError::NotAnInvitation)?;
        if encoded.len() > MAX_ENCODED_BYTES {
            return Err(InvitationError::TooLong {
                field: "link",
                actual: encoded.len(),
                limit: MAX_ENCODED_BYTES,
            });
        }
        let bytes = unbase64url(encoded).ok_or(InvitationError::Malformed)?;
        let (&header, body) = bytes
            .split_first()
            .ok_or(InvitationError::Truncated { field: "header" })?;
        if header & VERSION_MASK != VERSION {
            return Err(InvitationError::UnsupportedVersion {
                actual: header & VERSION_MASK,
            });
        }
        let body = if header & FLAG_DEFLATE == 0 {
            body.to_vec()
        } else {
            inflate(body).ok_or(InvitationError::Decompression)?
        };
        Self::from_body(&body)
    }

    /// True when any advertised endpoint shares a subnet with one of `local`.
    ///
    /// Scanning a code produced on another network is the ordinary way an
    /// in-person share fails: the addresses are honest, they just name a
    /// network this device cannot reach, and the transfer stalls with nothing
    /// to explain it. Comparing the addresses already present answers exactly
    /// that question, and does it without the location permission every
    /// platform demands before it will name the current Wi-Fi network.
    #[must_use]
    pub fn shares_network_with(&self, local: &[IpAddr]) -> bool {
        self.peers
            .iter()
            .any(|peer| local.iter().any(|here| same_subnet(peer.ip(), *here)))
    }

    /// The info hash as the lowercase hex every torrent interface speaks.
    #[must_use]
    pub fn info_hash_hex(&self) -> String {
        use std::fmt::Write;

        self.info_hash.iter().fold(
            String::with_capacity(self.info_hash.len() * 2),
            |mut out, byte| {
                // Writing into a String cannot fail.
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
    }

    fn body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&self.info_hash);
        out.extend_from_slice(&self.entries.to_be_bytes());
        out.extend_from_slice(&self.issued_at_secs.to_be_bytes());
        put_text(&mut out, &self.name);
        put_text(&mut out, &self.owner);
        let peers = &self.peers[..self.peers.len().min(MAX_PEERS)];
        // MAX_PEERS is far below u8::MAX, so this cannot truncate.
        out.push(u8::try_from(peers.len()).unwrap_or(u8::MAX));
        for peer in peers {
            match peer.ip() {
                IpAddr::V4(address) => {
                    out.push(TAG_V4);
                    out.extend_from_slice(&address.octets());
                }
                IpAddr::V6(address) => {
                    out.push(TAG_V6);
                    out.extend_from_slice(&address.octets());
                }
            }
            out.extend_from_slice(&peer.port().to_be_bytes());
        }
        out
    }

    fn from_body(body: &[u8]) -> Result<Self, InvitationError> {
        let mut reader = Reader::new(body);
        let info_hash = reader.array::<20>("info hash")?;
        let entries = u32::from_be_bytes(reader.array::<4>("entry count")?);
        let issued_at_secs = u32::from_be_bytes(reader.array::<4>("issued at")?);
        let name = reader.text("name")?;
        let owner = reader.text("owner")?;
        let count = usize::from(reader.byte("peer count")?);
        if count > MAX_PEERS {
            return Err(InvitationError::TooManyPeers { actual: count });
        }
        let mut peers = Vec::with_capacity(count);
        for _ in 0..count {
            let ip = match reader.byte("peer address tag")? {
                TAG_V4 => IpAddr::V4(Ipv4Addr::from(reader.array::<4>("peer address")?)),
                TAG_V6 => IpAddr::V6(Ipv6Addr::from(reader.array::<16>("peer address")?)),
                actual => return Err(InvitationError::UnknownAddressTag { actual }),
            };
            let port = u16::from_be_bytes(reader.array::<2>("peer port")?);
            peers.push(SocketAddr::new(ip, port));
        }
        reader.finish()?;
        Ok(Self {
            info_hash,
            name,
            owner,
            entries,
            issued_at_secs,
            peers,
        })
    }
}

/// Whether two addresses plausibly sit on one local network.
///
/// A /24 for IPv4 and a /64 for IPv6 are the prefixes home networks are
/// actually handed out on. This is a usability check, not a security boundary:
/// a false positive costs a missing warning, never access.
fn same_subnet(left: IpAddr, right: IpAddr) -> bool {
    match (left, right) {
        (IpAddr::V4(left), IpAddr::V4(right)) => left.octets()[..3] == right.octets()[..3],
        (IpAddr::V6(left), IpAddr::V6(right)) => left.octets()[..8] == right.octets()[..8],
        _ => false,
    }
}

/// Writes a length-prefixed string, truncated on a character boundary.
///
/// Truncation keeps a long collection name from pushing the QR past what a
/// camera can resolve. It happens on the sending side, where the whole string
/// is known, so a reader never sees a partial code point.
fn put_text(out: &mut Vec<u8>, text: &str) {
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    // MAX_TEXT_BYTES is far below u8::MAX, so this cannot truncate.
    out.push(u8::try_from(end).unwrap_or(u8::MAX));
    out.extend_from_slice(&text.as_bytes()[..end]);
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize, field: &'static str) -> Result<&'a [u8], InvitationError> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(InvitationError::Truncated { field })?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self, field: &'static str) -> Result<[u8; N], InvitationError> {
        let taken = self.take(N, field)?;
        let mut out = [0u8; N];
        out.copy_from_slice(taken);
        Ok(out)
    }

    fn byte(&mut self, field: &'static str) -> Result<u8, InvitationError> {
        Ok(self.take(1, field)?[0])
    }

    fn text(&mut self, field: &'static str) -> Result<String, InvitationError> {
        let length = usize::from(self.byte(field)?);
        if length > MAX_TEXT_BYTES {
            return Err(InvitationError::TooLong {
                field,
                actual: length,
                limit: MAX_TEXT_BYTES,
            });
        }
        let taken = self.take(length, field)?;
        String::from_utf8(taken.to_vec()).map_err(|_| InvitationError::NotUtf8 { field })
    }

    fn finish(self) -> Result<(), InvitationError> {
        let left = self.bytes.len() - self.at;
        if left == 0 {
            Ok(())
        } else {
            Err(InvitationError::Trailing { actual: left })
        }
    }
}

fn deflate(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;

    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(bytes).ok()?;
    encoder.finish().ok()
}

fn inflate(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;

    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(bytes)
        // Bounded so a small code cannot ask this device for unbounded memory.
        .take(MAX_ENCODED_BYTES as u64)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Unpadded base64url, written out rather than pulled in.
///
/// The alphabet is six lines of table lookup and keeps this crate free of a
/// dependency for something whose exact output is part of the wire contract.
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut packed = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            packed |= u32::from(*byte) << (16 - 8 * index);
        }
        // One output character per 6 bits present, which for a 1- or 2-byte
        // tail is fewer than four — that is what "unpadded" means here.
        for index in 0..=chunk.len() {
            let slot = (packed >> (18 - 6 * index)) & 0b11_1111;
            out.push(ALPHABET[slot as usize] as char);
        }
    }
    out
}

fn unbase64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut packed = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let slot = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        packed = (packed << 6) | u32::from(slot);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            // Masked to a byte first: the shift leaves the consumed bits in
            // the low eight, and everything above them belongs to the next
            // output byte rather than this one.
            out.push(((packed >> bits) & 0xff) as u8);
        }
    }
    // Whatever is left is the tail's padding bits. They must be zero, or the
    // text was not produced by the encoder above.
    (packed & ((1 << bits) - 1) == 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invitation() -> Invitation {
        Invitation {
            info_hash: [0xab; 20],
            name: "Attic Boxes".to_owned(),
            owner: "Ada's iPhone".to_owned(),
            entries: 15,
            issued_at_secs: 1_788_601_577,
            peers: vec![
                "192.168.0.100:6881".parse().unwrap(),
                "[2a04:cec0:3e6:692b:18bd:59e5:c055:f083]:6881"
                    .parse()
                    .unwrap(),
            ],
        }
    }

    #[test]
    fn an_invitation_survives_a_round_trip_unchanged() {
        let original = invitation();
        let decoded = Invitation::decode(&original.encode()).expect("decodes");
        assert_eq!(decoded, original);
    }

    #[test]
    fn every_field_is_carried_not_merely_the_identity() {
        let decoded = Invitation::decode(&invitation().encode()).expect("decodes");
        assert_eq!(decoded.name, "Attic Boxes");
        assert_eq!(decoded.owner, "Ada's iPhone");
        assert_eq!(decoded.entries, 15);
        assert_eq!(decoded.issued_at_secs, 1_788_601_577);
        assert_eq!(decoded.peers.len(), 2);
        assert_eq!(decoded.info_hash_hex(), "ab".repeat(20));
    }

    #[test]
    fn the_link_is_app_routable_and_scannably_short() {
        let link = invitation().encode();
        assert!(link.starts_with(INVITATION_PREFIX), "{link}");
        // A QR a phone can resolve across a table, not a wall of data.
        assert!(link.len() < 200, "{} chars: {link}", link.len());
    }

    /// The prefix is necessarily written down twice — once here, once in
    /// Dart's `invitationPrefix`, because the scanner must recognise a code
    /// before the backend has seen it. Pin the Rust side so a change here
    /// fails loudly rather than silently making every shared code unscannable.
    #[test]
    fn the_prefix_matches_the_one_the_scanner_looks_for() {
        assert_eq!(INVITATION_PREFIX, "portalis://c/");
    }

    #[test]
    fn a_link_of_another_scheme_is_not_mistaken_for_an_invitation() {
        for link in [
            "magnet:?xt=urn:btih:abcd",
            "portalis://import?magnet=magnet:?xt=urn:btih:abcd",
            "https://example.test/c/AAAA",
            "",
        ] {
            assert_eq!(
                Invitation::decode(link),
                Err(InvitationError::NotAnInvitation),
                "{link}"
            );
        }
    }

    #[test]
    fn a_future_version_is_refused_rather_than_guessed_at() {
        let mut bytes = vec![VERSION + 1];
        bytes.extend_from_slice(&invitation().body());
        let link = format!("{INVITATION_PREFIX}{}", base64url(&bytes));
        assert_eq!(
            Invitation::decode(&link),
            Err(InvitationError::UnsupportedVersion {
                actual: VERSION + 1
            })
        );
    }

    #[test]
    fn a_truncated_invitation_is_refused_rather_than_half_believed() {
        let link = invitation().encode();
        // Cut inside the payload, keeping the prefix intact.
        let cut = &link[..link.len() - 8];
        assert!(
            matches!(
                Invitation::decode(cut),
                Err(InvitationError::Truncated { .. } | InvitationError::Decompression)
            ),
            "{:?}",
            Invitation::decode(cut)
        );
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = vec![VERSION];
        bytes.extend_from_slice(&invitation().body());
        bytes.push(0);
        let link = format!("{INVITATION_PREFIX}{}", base64url(&bytes));
        assert_eq!(
            Invitation::decode(&link),
            Err(InvitationError::Trailing { actual: 1 })
        );
    }

    #[test]
    fn a_name_longer_than_the_limit_is_truncated_on_a_character_boundary() {
        let long = Invitation {
            // Multi-byte, so a naive byte cut would split a code point.
            name: "é".repeat(MAX_TEXT_BYTES),
            ..invitation()
        };
        let decoded = Invitation::decode(&long.encode()).expect("decodes");
        assert!(decoded.name.len() <= MAX_TEXT_BYTES);
        assert!(long.name.starts_with(&decoded.name));
    }

    #[test]
    fn more_peers_than_the_limit_are_dropped_rather_than_producing_an_unreadable_code() {
        let crowded = Invitation {
            peers: (0..MAX_PEERS + 10)
                .map(|index| {
                    let last = u8::try_from(index).expect("well under 255");
                    SocketAddr::new(Ipv4Addr::new(10, 0, 0, last).into(), 6881)
                })
                .collect(),
            ..invitation()
        };
        let decoded = Invitation::decode(&crowded.encode()).expect("decodes");
        assert_eq!(decoded.peers.len(), MAX_PEERS);
    }

    #[test]
    fn a_code_from_this_network_is_told_apart_from_one_that_is_not() {
        let here = invitation();
        assert!(here.shares_network_with(&["192.168.0.104".parse().unwrap()]));
        assert!(!here.shares_network_with(&["10.153.41.26".parse().unwrap()]));
        // The real failure this catches: the sender moved from .0.x to .1.x
        // between producing the code and it being scanned.
        assert!(!here.shares_network_with(&["192.168.1.104".parse().unwrap()]));
    }

    #[test]
    fn an_ipv6_peer_matches_its_own_prefix_only() {
        let here = invitation();
        assert!(
            here.shares_network_with(&["2a04:cec0:3e6:692b:9988:7e75:fee0:6739".parse().unwrap()])
        );
        assert!(
            !here.shares_network_with(&["2a04:cec0:3e6:0000:9988:7e75:fee0:6739".parse().unwrap()])
        );
    }

    #[test]
    fn a_multihomed_device_matches_on_any_shared_network() {
        // The sender advertised a VPN address and a LAN address; the receiver
        // only shares the LAN one. That is still the same network.
        let here = invitation();
        assert!(here.shares_network_with(&[
            "172.16.9.9".parse().unwrap(),
            "192.168.0.104".parse().unwrap(),
        ]));
    }

    #[test]
    fn an_invitation_with_no_peers_is_valid_and_shares_no_network() {
        let lonely = Invitation {
            peers: Vec::new(),
            ..invitation()
        };
        let decoded = Invitation::decode(&lonely.encode()).expect("decodes");
        assert!(decoded.peers.is_empty());
        assert!(!decoded.shares_network_with(&["192.168.0.104".parse().unwrap()]));
    }

    #[test]
    fn text_outside_the_base64url_alphabet_is_refused() {
        assert_eq!(
            Invitation::decode(&format!("{INVITATION_PREFIX}not base64!")),
            Err(InvitationError::Malformed)
        );
    }

    /// The bounds above are enforced on the sending side, so reaching these
    /// arms takes a hand-built body — which is exactly what a hostile code is.
    /// They are the difference between refusing bad input and allocating for
    /// it, so they are worth reaching deliberately.
    fn hostile(body: &[u8]) -> Result<Invitation, InvitationError> {
        let mut bytes = vec![VERSION];
        bytes.extend_from_slice(body);
        Invitation::decode(&format!("{INVITATION_PREFIX}{}", base64url(&bytes)))
    }

    #[test]
    fn an_oversized_link_is_refused_before_it_is_decoded() {
        let link = format!("{}{}", INVITATION_PREFIX, "A".repeat(MAX_ENCODED_BYTES + 1));
        assert_eq!(
            Invitation::decode(&link),
            Err(InvitationError::TooLong {
                field: "link",
                actual: MAX_ENCODED_BYTES + 1,
                limit: MAX_ENCODED_BYTES,
            })
        );
    }

    #[test]
    fn a_declared_text_length_over_the_limit_is_refused() {
        let mut body = Vec::new();
        body.extend_from_slice(&[0u8; 20]);
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        // A name claiming more bytes than any honest encoder would write.
        body.push(u8::try_from(MAX_TEXT_BYTES).unwrap() + 1);
        assert_eq!(
            hostile(&body),
            Err(InvitationError::TooLong {
                field: "name",
                actual: MAX_TEXT_BYTES + 1,
                limit: MAX_TEXT_BYTES,
            })
        );
    }

    #[test]
    fn a_declared_peer_count_over_the_limit_is_refused_before_allocating() {
        let mut body = Vec::new();
        body.extend_from_slice(&[0u8; 20]);
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.push(0); // empty name
        body.push(0); // empty owner
        body.push(u8::try_from(MAX_PEERS).unwrap() + 1);
        assert_eq!(
            hostile(&body),
            Err(InvitationError::TooManyPeers {
                actual: MAX_PEERS + 1
            })
        );
    }

    #[test]
    fn an_unknown_address_family_is_refused_rather_than_assumed_to_be_v4() {
        let mut body = Vec::new();
        body.extend_from_slice(&[0u8; 20]);
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.push(0);
        body.push(0);
        body.push(1); // one peer
        body.push(9); // neither 4 nor 6
        assert_eq!(
            hostile(&body),
            Err(InvitationError::UnknownAddressTag { actual: 9 })
        );
    }

    #[test]
    fn base64url_round_trips_every_tail_length() {
        for length in 0..8u8 {
            let bytes: Vec<u8> = (0..length).map(|index| index.wrapping_mul(37)).collect();
            let text = base64url(&bytes);
            assert!(!text.contains('='), "unpadded: {text}");
            assert_eq!(unbase64url(&text).as_deref(), Some(bytes.as_slice()));
        }
    }

    #[test]
    fn compression_is_used_only_when_it_actually_shrinks_the_payload() {
        // A long, highly repetitive name is the case deflate wins on.
        let repetitive = Invitation {
            name: "ab".repeat(MAX_TEXT_BYTES / 2),
            ..invitation()
        };
        let encoded = repetitive.encode();
        let bytes = unbase64url(encoded.strip_prefix(INVITATION_PREFIX).unwrap()).unwrap();
        assert_eq!(bytes[0] & FLAG_DEFLATE, FLAG_DEFLATE, "should compress");
        assert_eq!(Invitation::decode(&encoded).unwrap(), repetitive);

        // A short one with an incompressible hash is the case it loses on, and
        // the encoder must then ship the plain body rather than a larger one.
        let plain = invitation().encode();
        let bytes = unbase64url(plain.strip_prefix(INVITATION_PREFIX).unwrap()).unwrap();
        assert_eq!(bytes[0] & FLAG_DEFLATE, 0, "should not compress");
    }
}
