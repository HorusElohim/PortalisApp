# ADR-0001 — Single API seam: AppSnapshot + Command

**Status:** proposed
**Date:** 2026-08-16
**Superseded by:** —

## Context
The Flutter↔Rust boundary had **two live API surfaces** at once: `src/api.rs`
(2780 LOC, FRB-generated, leaking internal function signatures across the FFI one
by one) and `src/portalis_api.rs` (677 LOC, hand-written v3), plus a dead Dart
mirror `nexus/bridge/torrent.dart`. The audit's #1 cause of "I can't find where
backend↔frontend happens" was that there were literally two live answers. Path A
(generated bindings) is not a designed contract; Path B (state in, commands out)
is one.

## Decision
The Flutter↔Rust boundary is **exactly one surface**: the UI *reads* one
`AppSnapshot` and *sends* one `Command`. The older auto-generated `api.rs` path
and its Dart mirror (`bridge/torrent.dart`) are **deleted**.

## Why (pattern)
**Facade + Command pattern.** A Facade collapses many internal operations behind
one narrow, stable contract — one door, not two. Modeling the write side as a
single `Command` (a closed enum of intents) makes the interface a designed
contract rather than a bag of exported functions, and makes every call auditable
and testable at one seam. This is the leanest stance: one seam a new dev or agent
can find in one grep.

## Consequences
- Every feature talks to the backend the same way; adding a capability = one new
  `Command` variant + one field in `AppSnapshot`.
- The generated `api.rs` path, its Dart mirror, and the dead `torrent.dart`
  functions are removed (see ADR-0004 for the naming hazard they caused).
- The backend's internal function signatures no longer leak across the FFI; the
  contract is explicit and versionable.
- This is the concrete unblock for "where is the seam?" — one seam on the Rust
  side, symmetric with the one seam on the Dart side (ADR-0006).
