# ADR-0003 — Transport: Substrate redesigned as a Strategy with two real engines

**Status:** superseded
**Date:** 2026-08-16
**Superseded by:** [ADR-0003.5](0003.5-single-bittorrent-substrate-discovery-strategies.md)

> Superseded by ADR-0003.5: the online/offline goal remains, but it is served by
> one BitTorrent substrate with pluggable peer discovery rather than two
> payload engines.

## Context
`src/substrate.rs` defined a `Substrate` trait ("what moves the bytes") with **one
real engine (librqbit)** — flagged by the repo's own SPEC D15 as premature
abstraction (a trait with a single implementer is dead abstraction). But offline
sharing is a **genuinely different transport**, so a real second consumer is coming.
This was premature abstraction *for a real future need*, not needless abstraction —
the fix is to design it right, not delete it.

## Decision
Keep `Substrate` as a genuine high-level transport abstraction, but **redesign it**
to be a Strategy that swaps between **two real engines**:
- **Online:** BitTorrent / librqbit (swarm, relay/tracker-assisted).
- **Offline:** direct device-to-device (LAN / QUIC / mDNS), no coordination server.

## Why (pattern)
**Strategy pattern, justified by two concrete strategies.** A Strategy earns its
keep only when two (or more) real strategies exist and are chosen by context. Here
both branches ship: the app picks transport by context — internet available →
BitTorrent swarm; same LAN, no internet → direct device-to-device. This also makes
the symmetric-peer story concrete: offline peer-to-peer is two peers with no server
in the middle.

## Consequences
- The abstraction is no longer speculative — both strategies are real and tested.
- The app selects transport by runtime context, not by a compile-time fork.
- Offline BE-to-BE becomes two peers with no server in between (supports ADR-0007).
- Open transport details to resolve during implementation (not blockers): NAT
  traversal for the online swarm (librqbit vs iroh relay), and the offline discovery
  mechanism (mDNS is the leading LAN candidate).
