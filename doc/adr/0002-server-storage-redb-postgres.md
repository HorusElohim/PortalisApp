# ADR-0002 — Server storage: redb now, Postgres as the scale target, Mongo and the second in-memory engine deleted

**Status:** proposed
**Date:** 2026-08-16
**Superseded by:** —

## Context
The coordination node persisted to **two** backends at once — embedded `redb`
and MongoDB (`crates/storage/src/mongo/`, ~1400 LOC plus a Docker/replica-set test
dependency). Maintaining two storage engines for zero shipped deployments is the
exact "parallel variation" trap. The Nexus server is a **coordination plane, not
a data plane**: media never touches it; it stores only small signed/encrypted
records it cannot even read.

The same trap exists one layer up from the fault-injectable test fakes.
`NexusStore` (`apps/server/src/store.rs`) is an enum of two variants:
`Embedded` (the real redb engine, `crates/storage/src/embedded.rs`) and
`Memory(Box<InMemoryIdentities>)`, presenting `server-core`'s in-memory test
double (`crates/server-core/src/memory.rs`) as an alternate, interchangeable
*production* backend — the default for local runs, the demo, and `NexusStore`
itself. `Embedded` already has its own in-memory mode —
`Embedded::in_memory()` runs the identical redb engine against
`redb::backends::InMemoryBackend`, not a file — so `NexusStore::Memory` is not
filling a gap, it is a second, independently-written rulebook standing in for
the one engine that actually ships. The storage module's own doc comment
names the failure mode this produces: two engines "differed about whether
revoking a device twice moved the revocation time, and only a conformance
suite noticed." `crates/storage/tests/conformance.rs` compounds this by
running its whole suite against `Engine::Memory` too, certifying a backend
nothing ever deploys.

This is distinct from `InMemoryIdentities`'s other job: inside `server-core`
itself, `friends.rs`, `identity.rs`, `envelopes.rs`, and `share.rs` wrap it in
small `TestStore`/`FaultyStore` fault-injection shims to unit-test the
*service* layer (validation, business rules) in isolation from any storage
engine, real or not. That is an ordinary test fake for code that owns no
persistence of its own — `server-core` cannot depend on `storage` (the
dependency runs the other way: `storage` implements `server-core`'s
repository traits), so this usage stays.

## Decision
The coordination node persists to embedded **redb** for v1, for both durable
and in-memory use. When a single coordination node genuinely saturates, the
scale successor is **PostgreSQL** — not MongoDB.

- **Delete `NexusStore::Memory`** (`apps/server/src/store.rs`) and its
  `InMemoryIdentities` import. `NexusStore` becomes a thin wrapper around
  `Embedded`; its `Default` and any local-run/demo path use
  `Embedded::in_memory()` instead, which runs the exact engine production
  runs, just without a file.
- **Delete `Engine::Memory`** from `crates/storage/tests/conformance.rs`. The
  suite conforms the engine that ships (`Embedded`, in both its file-backed
  and in-memory forms belong here if useful) — it stops certifying a backend
  nothing deploys.
- `InMemoryIdentities`, `FixedClock`, and `ScriptedRandom` in
  `crates/server-core/src/memory.rs` **stay**: they are `server-core`'s own
  fault-injectable test fakes for its service-layer unit tests
  (`friends.rs`, `identity.rs`, `envelopes.rs`, `share.rs`), not a competing
  production storage engine, and removing them would force a dependency
  cycle between `server-core` and `storage` for no gain.

## Why (pattern)
**Strategy pattern with one chosen strategy.** A Storage strategy interface keeps
the "when do we need Postgres" decision a real, signal-driven trigger rather than
speculation, but for v1 only **one** concrete strategy (redb) is wired in as the
actual application backend — in both its durable and in-memory forms. Postgres
fits signed key→record rows better than Mongo (relational, real ACID for the atomic
user+device registration) and has a sane vertical-then-horizontal scale path. Mongo
solved a scaling problem this server will not have, at permanent operational cost.
`NexusStore::Memory` solved a problem (a fast, file-free run mode) that
`Embedded::in_memory()` already solves, with the added cost of a second
rulebook that can drift from the one that ships. `server-core`'s test fakes are
a different pattern — **Test Double**, scoped to one crate's own unit tests —
and are not what this decision is about.

## Consequences
- Tests run anywhere (no Docker, no file); self-hosters get a single directory
  of files.
- "When do we need Postgres" is a measurable trigger (node saturation), not
  guesswork.
- Mongo code and its test dependency are already removed (prior commit).
  `NexusStore::Memory` and `Engine::Memory` are removed: the only backend
  `NexusStore` can be is `Embedded` (file-backed or `in_memory()`), so a
  service running on storage nobody expected is no longer possible by
  construction.
- `apps/server/src/store.rs` tests that exercised `NexusStore::Memory`
  (e.g. `a_forced_fault_reaches_the_in_memory_backend_only`) move onto
  `Embedded::in_memory()`; its `set_unavailable`/`set_devices_unavailable`
  fault hooks either move onto `Embedded` if it needs them for that test, or
  the test is redesigned around what `Embedded::in_memory()` can actually
  fail on.
- `server-core`'s own unit tests are unaffected: `InMemoryIdentities` keeps
  serving as the service layer's fault-injectable fake.
- The Storage seam is a strategy interface, so adding Postgres later is additive,
  not a rewrite.
