# ADR-0016 — Rust-owned app contract with a thin Flutter renderer

**Status:** proposed
**Date:** 2026-09-01
**Supersedes:** ADR-0006 and ADR-0010
**Superseded by:** —

## Context

Rust owns durable state, transfer measurement, lifecycle, and trust, but Flutter
still reparses lifecycle strings and derives sharing eligibility, completion,
movement, aggregate activity, pending counts, fallback progress, completion
edges, and command validity. Several wrapper models sit between generated DTOs
and widgets. The reported `0 items` defect is a direct consequence: Rust already
projects the authoritative summary count, while a Dart presentation wrapper
counts an absent detail list instead.

## Decision

Rust exposes one deliberately app-renderable, typed contract; Flutter renders it.

- Replace string lifecycle/nature/role values with generated typed Rust enums.
- Project explicit Rust-owned capabilities and facts needed by UI decisions,
  including share/select/add/pause/resume/delete eligibility, completion,
  sharing/movement state, pending count, authoritative progress, and aggregate
  engine activity.
- Emit typed lifecycle events, including receiver completion. Flutter delivers
  platform notifications but does not infer completion edges from snapshots.
- Replace the string-plus-optional-fields command envelope with generated typed
  commands or narrow typed bridge functions so invalid combinations cannot be
  constructed.
- Preserve tiered summary, selected-detail, append-only-history, peer-history,
  and event streams; thin does not mean one oversized snapshot.
- Use generated Rust DTOs directly behind one fakeable Flutter gateway.
- Retire field-for-field and decision-bearing wrappers once consumers migrate.
- Keep only presentation formatting, colors, icons, layout, accessibility text,
  and transient widget state in Flutter.

## Consequences

- Rust is the sole authority for domain and application decisions.
- Flutter cannot drift from backend lifecycle vocabulary or recompute counts.
- Bridge changes and generated bindings become larger but explicit atomic edits.
- The old wrappers are removed only after all consumers use the generated types.

## Acceptance verification

- [x] Summary rows render Rust `entries` with no detail subscription and never
      contradict collection detail — `subtitle reports the summary item count
      without a detail` (collection_test.dart), fixed independently and
      landed ahead of this ADR's full scope; also `metadata_resolution_is_a_typed_preparing_fact`
      confirms the typed contract projects the fact directly.
- [x] Flutter contains no lifecycle parser or domain capability derivation for
      collection lifecycle/nature/role. `AppCollectionLifecycle`,
      `AppCollectionNature`, `AppCollectionRole`, `AppCollectionCapabilities`,
      and `AppCollectionFacts` are generated Rust enums/structs; Flutter
      compares and formats them, computing nothing that was previously a
      `Status`/`Nature`/`Role` string comparison. `collection_state_test.dart`
      and `collection.dart` no longer contain a `parse`/`wire` string
      round-trip for these three contracts.
- [x] Aggregate engine activity comes from Rust — `AppActivity` is now a
      generated bridge struct aggregated once in `app_activity()`
      (`portalis_api.rs`) and read directly by `AppController.activity`;
      Flutter's per-collection summing loop is gone.
      `snapshot_activity_aggregates_only_moving_collections` proves only
      collections with a live transfer contribute.
- [ ] Completion notifications consume a Rust event — not yet migrated;
      `TransferCompletionObserver` still diffs successive snapshots itself,
      though it now reads `AppCollectionRole.member` rather than a string.
- [ ] Invalid command shapes are unrepresentable at the bridge boundary — the
      string-`kind`-plus-optional-fields `AppCommand`/`EngineCommand` envelope
      is unchanged.
- [x] Summary/detail/history/event cadence remains tiered — untouched by this
      slice.
- [x] Wrapper deletion reduces code without weakening widget test seams —
      `CollectionState`/`CollectionNature`/`CollectionRole` string parsers and
      their `parse`/`wire` machinery in `collection_state.dart` are gone,
      replaced by typedefs onto the generated enums plus one presentation
      `label()` extension; all existing widget test seams (`buildCollection`,
      `_FixedSource`, etc.) still compile and pass unchanged in shape.
- [x] Generated bridge round-trip and full Flutter/Rust suites pass —
      284/284 backend tests (0 failed, 1 ignored), clippy `-D warnings` clean,
      `cargo fmt --check` clean; 180/180 Flutter tests, `flutter analyze`
      clean; FRB regenerated via `tool/frb_build.sh --codegen-only --force-frb`.

This ADR's typed lifecycle/nature/role/capabilities/facts slice is complete
and verified. Aggregate activity, typed completion events, and a typed
command envelope remain open follow-up work before this ADR can be marked
`accepted`; status stays `proposed`.
