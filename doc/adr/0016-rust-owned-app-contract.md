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

- [ ] Summary rows render Rust `entries` with no detail subscription and never
      contradict collection detail.
- [ ] Flutter contains no lifecycle parser or domain capability derivation.
- [ ] Aggregate engine activity comes from Rust.
- [ ] Completion notifications consume a Rust event.
- [ ] Invalid command shapes are unrepresentable at the bridge boundary.
- [ ] Summary/detail/history/event cadence remains tiered.
- [ ] Wrapper deletion reduces code without weakening widget test seams.
- [ ] Generated bridge round-trip and full Flutter/Rust suites pass.
