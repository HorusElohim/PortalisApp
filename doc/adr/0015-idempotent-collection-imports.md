# ADR-0015 — Make collection imports idempotent in Rust

**Status:** proposed
**Date:** 2026-09-01
**Superseded by:** —

## Context

The OS may deliver the same deep link more than once or deliver the initial link
and stream event concurrently. Flutter currently remembers the last handled URI
only after awaiting an import, so concurrent deliveries can both issue commands.
UI-side deduplication cannot protect other import callers, process restarts, or
two syntactically different sources naming the same torrent.

## Decision

Rust owns import identity and idempotency.

- Canonicalize the source and derive the durable torrent identity as soon as its
  info hash is available.
- Admit at most one live/durable collection for the same import identity unless
  the product explicitly supports aliases later.
- Concurrent equivalent imports share one in-flight operation and return the
  same collection handle/result.
- A repeated import after restart returns the existing collection rather than
  creating another row.
- Failed imports may be retried; failure never permanently poisons the identity.
- Flutter serializes deep-link navigation so one successful Rust result opens
  one route, but it is not the deduplication authority.

## Consequences

- Duplicate OS deliveries cannot create duplicate collections.
- Every import entry point receives the same semantics.
- Rust must maintain a small in-flight identity map in addition to durable
  uniqueness checks.

## Acceptance verification

- [ ] Two concurrent identical links return one collection.
- [ ] Equivalent magnet encodings resolve to one durable identity.
- [ ] Reimport after restart returns the existing collection.
- [ ] A failed import can be retried successfully.
- [ ] Flutter navigates once for duplicate delivery.
- [ ] Manual magnet and scanned Portalis links preserve explicit selection and
      never start a download without the Download action.
