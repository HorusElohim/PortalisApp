# ADR-0013 — Hydrate collection membership from signed revisions

**Status:** proposed
**Date:** 2026-09-01
**Superseded by:** —

## Context

Membership belongs to the signed collection revision. The durable store retains
that revision, and the collection model derives members from it. Startup
hydration currently reads only the revision number and initializes every
projected collection with an empty member list. No production path repopulates
that list later, so restart loses projected collaborators while the signed truth
remains on disk.

## Decision

Rust hydration reconstructs membership from the current verified revision.

- Load and decode the persisted current revision during collection hydration.
- Verify it through the same revision-chain authority used at admission; do not
  trust a second, unsigned membership cache.
- Derive the current member roots from the revision.
- Resolve known roots to process-local contact/member handles in Rust.
- Preserve unknown-but-authorized members as an explicit projected shape rather
  than silently dropping them.
- Populate the list projection before the first snapshot is emitted.
- Treat corrupt, conflicting, rollback, or future revision state as an explicit
  collection verification status, never as an empty successful membership.

## Consequences

- Collaborators survive restart and match signed durable state.
- Flutter renders membership directly and keeps no second member list.
- Commands that depend on membership use the same Rust authority as the UI.
- Hydration may perform additional bounded store reads before the first snapshot.

## Acceptance verification

- [ ] Owner and member collections restore all signed members after restart.
- [ ] Known contacts resolve to the expected process-local handles.
- [ ] Unknown authorized members are not silently erased.
- [ ] A newer accepted revision replaces the projected member set.
- [ ] Corrupt, rollback, and conflicting revisions project explicit failures.
- [ ] Bridge round-trip preserves the Rust-projected membership.
