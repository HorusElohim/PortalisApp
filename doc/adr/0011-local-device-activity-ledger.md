# ADR-0011 — Local device activity and app-run ledger

**Status:** proposed  
**Date:** 2026-08-29  

## Context

Portalis has no account or centralized telemetry identity. The User page is the
profile of this local cryptographic device identity. It currently derives
statistics in Flutter by summing collection snapshots. Those values have
incompatible meanings: uploaded bytes are process-scoped, while on-disk bytes
are durable holdings that can include earlier downloads and zero-copy sources.
Calling both “session” traffic is incorrect, and deleting a collection removes
it from the apparent lifetime total.

The backend already observes native network counters, lifecycle transitions,
transfer completion, current library state and the durable device identity. It
needs one local ledger that turns those facts into restart-safe device activity
without introducing automatic telemetry or frequent disk writes.

## Decision

Rust owns a durable local device-activity ledger and bounded app-run history.

- Schema 12 adds one cumulative `device_activity` record and `app_runs`, bounded
  to the latest 30 runs.
- A run begins when Nexus starts. An unfinished previous run is finalized as
  interrupted; graceful shutdown finalizes the current run as clean.
- Network counters are accumulated from backend-native fetched/uploaded
  counters with per-collection baselines. Counter decreases start a new
  segment. Zero-copy source reads are never counted as network downloads;
  bytes served from a zero-copy source are genuine network uploads and count.
- Activity is observed in memory on transfer polls, but persisted only at
  semantic checkpoints: app backgrounding, confirmed download completion,
  graceful shutdown, and an optional coarse safety interval.
- Current-library facts are derived from authoritative collection rows and live
  state rather than copied into the durable ledger.
- Flutter receives an on-demand `AppUserSummary` and bounded recent runs through
  `AppRepository`. It formats and renders those values but never computes
  durable totals or defines run boundaries.
- Migration from schema 11 starts honest empty tracking. It does not fabricate
  historical totals from bounded samples or collection-scoped peer history.
  The UI says “Tracked since”, not “Member since”.
- “Clear activity history” resets cumulative activity and recent runs while
  preserving device identity, nickname, settings, collections, and per-
  collection peer history.

## Privacy and retention

The ledger remains on this device. It is never sent to peers, included in a
manifest or share link, or uploaded automatically. Global activity records do
not contain filenames, paths, media metadata, peer addresses, client strings,
proxy credentials, signing secrets, or collection names. Explicit export, if
added later, must use a user-selected local share action.

## Consequences

- Session and lifetime network traffic survive restarts and collection deletion.
- Held bytes, catalog bytes, and network-received bytes remain separate facts.
- Background checkpoints bound mobile process-kill loss without writing every
  500 ms.
- A crash can lose activity since the last semantic/coarse checkpoint, but the
  next start labels the unfinished run interrupted instead of silently calling
  it clean.
- The signing secret remains confined to `identity.json` and never crosses FRB.
- Added schema, lifecycle and bridge tests must prove idempotent checkpoints,
  counter reset handling, interrupted-run recovery, schema-11 migration,
  clearing isolation, and secret/path/endpoint exclusion.

## Non-goals

This does not create a Portalis account, cloud backup, cross-device activity
sync, analytics service, or recoverable signing identity. It does not infer
pre-schema-12 history.

**References:** `portalis/rust/backend/src/nexus/store/schema.rs`,
`portalis/rust/backend/src/nexus/core/nexus.rs`,
`portalis/rust/backend/src/nexus/core/transfers.rs`,
`portalis/rust/backend/src/portalis_api.rs`, and
`portalis/lib/features/identity/presentation/user_screen.dart`.
