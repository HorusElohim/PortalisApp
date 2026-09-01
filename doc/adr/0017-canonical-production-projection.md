# ADR-0017 — One production projection path, tested through production

**Status:** proposed
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

- [ ] No projection builder exists solely for its own tests.
- [ ] Startup and live updates share canonical lifecycle/count/byte logic.
- [ ] Restart matrix covers native owner, unresolved torrent, metadata-ready
      torrent, selected subset, completed receiver, paused state, membership,
      timestamps, and missing zero-copy source.
- [ ] Bridge tests consume production state.
- [ ] Reinstating the former duplicate path makes at least one regression test
      fail.
