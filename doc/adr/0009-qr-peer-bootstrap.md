# ADR-0009 — QR-first peer bootstrap for offline sharing

**Status:** accepted  
**Date:** 2026-08-19  
**Accepted:** 2026-09-05  
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

## Acceptance verification

Each Consequence bullet, next to the test that proves it. All tests live in
`portalis/rust/backend/crates/protocol/src/format/invitation.rs` unless noted.

- **Offline sharing has a simple first step with no mandatory Nexus or
  internet** — `an_invitation_survives_a_round_trip_unchanged`,
  `every_field_is_carried_not_merely_the_identity`.
- **QR payloads stay compact by using binary encoding rather than verbose
  JSON** — `the_link_is_app_routable_and_scannably_short` (bounds a realistic
  invitation under 200 characters),
  `compression_is_used_only_when_it_actually_shrinks_the_payload`.
- **A QR scan bootstraps the same BitTorrent transfer semantics used online** —
  `a_scanned_invitation_is_unwrapped_into_the_magnet_the_import_path_speaks`
  and `a_source_that_is_not_an_invitation_passes_through_untouched`, both in
  `src/nexus/core/nexus.rs`: the envelope is unwrapped once at the import
  boundary, so resolution, acquisition, and persistence are unchanged.
- **QR presentation, encoding, decoding, validation, and peer-hint injection
  need focused tests** — `a_future_version_is_refused_rather_than_guessed_at`,
  `a_truncated_invitation_is_refused_rather_than_half_believed`,
  `trailing_bytes_are_refused`, `text_outside_the_base64url_alphabet_is_refused`,
  `a_link_of_another_scheme_is_not_mistaken_for_an_invitation`,
  `base64url_round_trips_every_tail_length`,
  `a_name_longer_than_the_limit_is_truncated_on_a_character_boundary`,
  `more_peers_than_the_limit_are_dropped_rather_than_producing_an_unreadable_code`,
  `only_a_real_info_hash_decodes` (in `src/nexus/core/nexus.rs`), and
  `portalis/test/collection_link_test.dart`.

The envelope is versioned from its first release
(`a_future_version_is_refused_rather_than_guessed_at`), and the scheme prefix
is pinned on both sides of the language boundary by
`the_prefix_matches_the_one_the_scanner_looks_for` against Dart's
`invitationPrefix`, because the scanner must recognise a code before the
backend has parsed it.

### Decided during implementation

The ADR left the binary encoding open. This implementation uses a hand-written
fixed-width/length-prefixed body, deflate-compressed *only when that actually
shrinks it*, then unpadded base64url — not CBOR. A 20-byte hash and packed
socket addresses are close to incompressible, so unconditional compression
regularly produced a larger payload than it consumed; the header records which
form was used so the reader never guesses.

Two fields were added beyond the ADR's list, both to answer questions a
receiver previously could not ask before the network replied:

- `entries`, so the import screen can lay out placeholders immediately.
- `issued_at_secs`, because peer addresses are only true of the network the
  sharing device was on when the code was produced.

The ADR's optional "capability needed to open the encrypted content" is **not**
carried. It remains out of the envelope, consistent with the security note
below that a QR held up to a camera is readable by everyone who can see it.

Reachability is answered by comparing the invitation's advertised addresses to
this device's own (`shares_network_with`, `/24` for IPv4 and `/64` for IPv6)
rather than by naming the Wi-Fi network: the address comparison needs no
location permission, and the two answers derive from the same interface
enumeration that produces the hints, so they cannot disagree. It is a
usability check, not a security boundary — a false positive costs a missing
warning, never access.

### Announced facts are not verified facts

An invitation's fields are the sending device's claims, read off a screen. The
receiving app keeps them strictly separate from what it has verified:

- The announced **name** is adopted as the collection's name, because a name is
  presentation with no correctness consequence, and the alternative was a row
  reading "Portalis collection import" until the swarm answered.
- The announced **item count** is shown only in the pre-import confirmation
  sheet, where it is visibly attributed to the sender. It is deliberately not
  merged into `CollectionState.entries`, which stays a verified count derived
  from resolved metadata — merging them would let a scanned code make the
  interface assert something untrue about content it has not seen.
- The **info hash** remains the only thing that decides what content is, and it
  is verified by the substrate exactly as before.

Validation for `ImportTorrent` accepts all three shapes the import path
understands (magnet, `.torrent` path, invitation). It runs *before* the
envelope is unwrapped, so a shape missing from the guard is refused before it
can reach the code that handles it — which is how the first build shipped with
every scanned code rejected. Covered by
`validation_accepts_every_shape_the_import_path_understands`.

Platform URL registration is part of the format's contract, not an
implementation detail: the OS routes by scheme *and* host, so a host the app
produces but does not register is delivered nowhere, with nothing in the app
able to observe it. Covered by
`every host the app can produce is registered on Android`
(`portalis/test/collection_link_test.dart`).

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
