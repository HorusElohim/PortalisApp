# ADR-0017 — One production projection path, tested through production

**Status:** accepted
**Date:** 2026-09-01
**Superseded by:** —

## Context

The repository contains a complete-looking `projection/build.rs` model and test
suite, while startup and live production projection are built separately in
`core/nexus.rs`, `core/torrents.rs`, and `core/transfers.rs`. Tests can therefore
prove a helper correct without exercising the implementation that ships. This
is especially dangerous for restart hydration, where entry totals, membership,
lifecycle, timestamps, and transfer facts converge.

## Decision

There is one production projection constructor/update path, and tests invoke it.

- Move reusable projection construction behind a production API used by startup
  hydration and live updates.
- Delete isolated builders and tests that are not called by production.
- Keep status/capability arithmetic in one Rust module.
- Test persisted restart scenarios by opening a real store through the
  production Nexus path, not by constructing parallel `CollectionFacts` values.
- Test bridge projection from production state for every bridged field.
- Maintain separate focused tests for pure formatting/encoding helpers only when
  those helpers are themselves used by production.

## Consequences

- A green projection test proves the shipping path.
- Restart and live projection cannot silently implement the same rule twice.
- Tests may require more realistic store fixtures but provide stronger evidence.

## Acceptance verification

- [x] No projection builder exists solely for its own tests —
      `nexus/projection/build.rs` (a parallel `CollectionFacts`/`collection()`
      constructor and its own 12-test suite, never called by production) is
      deleted. `nexus/projection/mod.rs` no longer exports `build`.
- [x] Startup and live updates share canonical lifecycle/count/byte logic —
      startup hydration, `create_collection`, and `import_torrent` all build
      their initial `CollectionState` through one new function,
      `project_stored_collection`, instead of three separate inline struct
      literals that had already drifted (hydration alone duplicated the
      entries/total_bytes selected-only arithmetic). `status_for`'s
      persisted-half argument construction is likewise unified behind
      `StatusFacts::from_stored`, replacing four separate inline
      constructions in `nexus.rs` and `torrents.rs` that previously could
      (and did) diverge.
- [x] Restart matrix covers native owner, unresolved torrent, metadata-ready
      torrent, selected subset, completed receiver, paused state, membership,
      timestamps, and missing zero-copy source — all via the real `Nexus`
      open/close/reopen path:
      `a_reopened_published_owner_collection_is_seeding_while_its_zero_copy_seed_rehydrates`
      (native owner), `a_torrent_import_is_a_durable_collection_before_downloading`
      (unresolved torrent), `a_torrent_source_resolves_into_a_selection_then_downloads_it`
      (metadata-ready + selected subset), `a_completed_receiver_import_is_available_when_the_app_reopens`
      (completed receiver), `pausing_a_collection_is_reported_at_once_and_survives_a_restart`
      (paused state), `the_first_snapshot_restores_every_signed_member_after_restart`
      (membership), `collection_edits_are_visible_immediately_and_survive_a_restart`
      (timestamps/name), and the new
      `a_reopened_owner_collection_with_a_missing_zero_copy_source_hydrates_as_unavailable`
      (missing zero-copy source — this test caught and fixed a real bug: local
      sources were unconditionally reported `available: true` regardless of
      whether the referenced file still existed).
- [x] Bridge tests consume production state — `portalis_api::tests` builds
      `PortalisState`/`CollectionState` values and calls the real `snapshot`/
      `collection_projection` functions; no bridge test constructs a
      hand-rolled `AppSnapshot`/`AppCollection` bypassing projection.
- [x] Reinstating the former duplicate path makes at least one regression test
      fail — confirmed directly: `a_native_collection_publishes_through_the_injected_zero_copy_substrate`
      failed with an entries/total_bytes mismatch when `torrents::republish`
      read torrent-import metadata for a native (non-torrent) collection,
      which was the exact class of divergence this ADR exists to close;
      restoring the source-kind guard made it pass again.

Verification: 275/275 backend tests (0 failed, 1 ignored), `cargo fmt --check`
and `cargo clippy --lib --tests -- -D warnings` clean.
