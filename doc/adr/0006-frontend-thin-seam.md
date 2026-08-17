# ADR-0006 — Frontend shape: one thin seam layer

**Status:** proposed
**Date:** 2026-08-16
**Superseded by:** —

## Context
The frontend is lean on dependencies (single ChangeNotifier state model, all calls
through FRB) but **deep on indirection**: a 4-level DTO wrapping stack
(generated `AppCollection` → `nexus/domain/app_state.dart` →
`nexus/data/collection_view.dart` → `features/collections/domain/collection.dart`)
plus a `AppRepository`/`FrbAppRepository` interface that is a pure 1:1 pass-through.
Tracing "where does this progress number come from?" is expensive.

## Decision
Generated bridge types serve as the domain types (the team already started this —
they deleted a 200-line hand-mirror). Keep **one** minimal repository/service
boundary for testability. **Delete** the 4-level DTO wrapping stack and the
redundant per-feature `domain/data` wrappers. Test by faking the snapshot stream at
the one seam.

## Why (pattern)
**Facade / single-seam (one testable boundary).** One minimal seam gives
test-substitutability (the value of the pass-through interface) without the cost of
four wrapping layers. This aligns with ADR-0001's single AppSnapshot+Command seam on
the Rust side — **one seam on each side of the FFI, symmetric.**

## Consequences
- Generated bridge types are the domain types; no per-feature re-wrapping.
- One repository/service seam is retained as the test boundary; fake the snapshot
  stream there.
- The redundant `domain/data` wrappers are removed, flattening the data path.
- Symmetric with ADR-0001: one seam on each side of the FFI.
