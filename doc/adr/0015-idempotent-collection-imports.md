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

- [x] Two concurrent identical links return one collection —
      `concurrent_identical_imports_return_one_collection`.
- [x] Equivalent magnet encodings resolve to one durable identity —
      `equivalent_magnet_encodings_resolve_to_one_durable_identity`.
- [x] Reimport after restart returns the existing collection —
      `reimport_after_restart_returns_the_existing_collection`.
- [x] A failed import can be retried successfully —
      `a_failed_import_can_be_retried_successfully`.
- [x] Flutter navigates once for duplicate delivery — `CollectionLinkReceiver`
      tracks in-flight import keys so a second concurrent delivery of the same
      URI is dropped before a second `importCollectionLink` call, rather than
      relying on the already-completed `_handled` check alone.
- [x] Manual magnet and scanned Portalis links preserve explicit selection and
      never start a download without the Download action — pre-existing
      behavior in `collection_link.dart`
      (`startCollectionLinkDownload`/`downloadSelection`), unaffected by this
      change and covered by `collection_link_test.dart`.

The status remains `proposed` until owner validation at merge, as required by
the ADR index convention.
