# ADR-0005 — Decision records: ADRs only + index, no living SPEC

**Status:** proposed
**Date:** 2026-08-16
**Superseded by:** —

## Context
This repo already rewrote its entire SPEC once because it "narrated how the plane
was built" instead of describing the system — the docs kept decaying into
archaeology (`portalis-nexus/SPEC.md` at a path that no longer exists;
`docs/torrent-engine.md` describing a DHT design that contradicts the current one).
The docs-drift-into-history failure mode is a known, recurring problem in this repo.

## Decision
Record every architectural decision as a **numbered, append-only, frozen ADR** under
`doc/adr/`, with a `doc/adr/README.md` index (number · title · status ·
superseded-by). **No central living SPEC.** Delete narrative/history docs.
**The code is the current-state map.**

## Why (pattern)
**Append-only decision records (ADRs) as a superset-replaceable log.** A frozen ADR
cannot drift — it is superseded, never edited — whereas a living SPEC is a single
mutable document that continuously rots. The one risk ("no single current-state
map") is covered by the index: the set of non-superseded ADRs *is* the map,
assembled from frozen parts. This kills the format that rots.

## Consequences
- The current-state map = the code + the index of non-superseded ADRs.
- New decisions are added as the next number; supersession points at the new one.
- Narrative/history docs are deleted rather than maintained.
- This is the concrete "never again" guardrail against docs becoming history.
