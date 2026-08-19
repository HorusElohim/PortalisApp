# ADR-0008 — One BitTorrent substrate, pluggable peer discovery

**Status:** proposed  
**Date:** 2026-08-19  
**Supersedes:** [ADR-0003](0003-transport-substrate-dual-engine.md)

## Context

ADR-0003 treated online and offline sharing as two transport engines. The
current code has one production `Substrate` implementation, `Torrents`, backed
by librqbit. librqbit already transfers pieces directly between peers and
accepts explicit `initial_peers`. DHTs, trackers, LAN discovery, and QR-supplied
addresses are peer-discovery mechanisms, not different payload transports.

A second payload engine would duplicate piece selection, choking, integrity,
and resume behavior for no user benefit. `Recorded` remains a test double, not
a second production engine.

## Decision

Keep one production `Substrate` and one payload engine: **BitTorrent via
librqbit**.

Make peer discovery/bootstrap the variable part:

- **Online:** librqbit's normal swarm discovery, such as trackers/DHT where enabled.
- **Offline or same-LAN:** explicit peer hints supplied by the caller, initially
  from the QR bootstrap defined by ADR-0009.
- **Later convenience:** LAN discovery such as mDNS may add peer hints without
  introducing another payload engine or changing the `Substrate` contract.

The collection and authorization layers remain above the substrate. Nexus may
coordinate signed metadata and membership, but media bytes move peer-to-peer
and never through Nexus.

## Why (pattern)

**Strategy at the discovery boundary, not the transport boundary.** The
strategies choose how a swarm learns peer addresses while one engine owns the
hard data-plane rules. This is the smallest seam with multiple real consumers
and avoids a speculative second transport.

## Consequences

- Same-LAN sharing can work without internet when a reachable peer address is
  bootstrapped.
- Online and offline transfers share verification, piece selection, resume,
  pause, and release behavior.
- A public address is only a hint, not a reachability guarantee; Nexus or a
  future relay may help when direct connectivity fails.
- ADR-0003 is retained as history and superseded by this decision.
- Tests should cover discovery/bootstrap inputs separately from payload behavior.

## Non-goals

This ADR does not define the QR wire format, key packaging, or invitation UX;
those are ADR-0009.

## Implementation boundary

The exact peer-hint DTO and librqbit integration point remain implementation
details. They must preserve a versioned boundary and keep discovery policy out
of collection/domain rules.

**References:** `portalis/rust/backend/src/substrate.rs`,
`portalis/rust/backend/src/torrent.rs` (`initial_peers`), and
`portalis/rust/backend/docs/torrent-engine.md`.
