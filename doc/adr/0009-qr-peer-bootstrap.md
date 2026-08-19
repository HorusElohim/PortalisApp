# ADR-0009 — QR-first peer bootstrap for offline sharing

**Status:** proposed  
**Date:** 2026-08-19  
**Related:** [ADR-0003.5](0003.5-single-bittorrent-substrate-discovery-strategies.md), [ADR-0007](0007-symmetric-peer-topology.md)

## Context

A same-LAN transfer needs a way to learn a peer's reachable address and the
content it should request. BitTorrent already accepts explicit peer addresses;
the missing piece is a small, intentional bootstrap channel.

A QR scan is a useful first channel because it is local, explicit, and user
mediated. It can carry a compact invitation rather than requiring a server,
DHT rendezvous, or an always-on discovery service. It should not become a
second transport or an authorization ambiguity.

## Decision

Use a **versioned `portalis:` invitation URI** as the first offline bootstrap
format. Encode a compact binary envelope and render its base64url or base32
representation as a QR code.

The envelope may contain:

- protocol/version and envelope type;
- torrent info-hash or other content identifier;
- display name or a bounded name hint;
- LAN-first peer hints, with public endpoint hints optional;
- the capability needed to open the encrypted content, when the invitation is
  intentionally an authorization grant.

The receiver validates the version, bounds, identifier, and peer-hint fields,
then supplies the hints to the single BitTorrent substrate from ADR-0003.5.

The first UX is QR-first and in-person. Encryption of the entire URI is not a
requirement for an in-person scan: the scan itself is the deliberate channel.
If pasteable links are added later, their confidentiality and key placement
must be specified by a new or superseding ADR rather than assumed.

## Why (pattern)

**Bootstrap protocol + capability token.** The URI is a small versioned
bootstrap protocol; the content capability makes the invitation explicit and
prevents peer discovery from being mistaken for authorization. This keeps
transport, discovery, and access control separate.

## Security and privacy

- Peer hints are untrusted network input and must be validated and bounded.
- A peer address alone grants no access; the capability/capsule key remains the
  authorization material.
- Prefer LAN addresses for same-room sharing. A public address is only a
  best-effort hint and may be stale, unroutable, or behind NAT.
- Do not put long-lived private identity keys in the QR envelope.
- Do not claim that QR encryption protects an in-person scan from its viewer.
- If a link is copied or forwarded, treat it as a bearer capability and design
  expiry, revocation, or key rotation before calling it safe for that use.

## Consequences

- Offline sharing has a simple first step with no mandatory Nexus or internet.
- QR payloads stay compact by using binary encoding rather than verbose JSON.
- A QR scan bootstraps the same BitTorrent transfer semantics used online.
- mDNS or another discovery convenience can be added later without changing
  the content-transfer engine.
- QR presentation, encoding, decoding, validation, and peer-hint injection
  need focused tests.

## Non-goals

This ADR does not select CBOR versus another binary encoding, define a final
cryptographic envelope, promise NAT traversal, or add mDNS. Those choices need
live implementation evidence and may require a follow-up ADR.

## Migration and compatibility

The envelope must be versioned from its first release. Older clients that do
not understand `portalis:` invitations may continue using existing torrent and
magnet flows; they must not interpret an unknown envelope as a torrent path.

**References:** `portalis/rust/backend/src/torrent.rs` (`initial_peers`),
`portalis/rust/backend/src/substrate.rs`, and
`portalis/rust/backend/docs/torrent-engine.md`.
