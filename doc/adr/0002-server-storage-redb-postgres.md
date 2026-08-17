# ADR-0002 — Server storage: redb now, Postgres as the scale target, Mongo deleted

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

## Decision
The coordination node persists to embedded **redb** for v1. When a single
coordination node genuinely saturates, the scale successor is **PostgreSQL** —
not MongoDB. **Delete the MongoDB backend now** (the `mongo` module and its
Docker/replica-set test dependency).

## Why (pattern)
**Strategy pattern with one chosen strategy.** A Storage strategy interface keeps
the "when do we need Postgres" decision a real, signal-driven trigger rather than
speculation, but for v1 only **one** concrete strategy (redb) is wired in. Postgres
fits signed key→record rows better than Mongo (relational, real ACID for the atomic
user+device registration) and has a sane vertical-then-horizontal scale path. Mongo
solved a scaling problem this server will not have, at permanent operational cost.

## Consequences
- Tests run anywhere (no Docker); self-hosters get a single file.
- "When do we need Postgres" is a measurable trigger (node saturation), not
  guesswork.
- Mongo code and its test dependency are removed; one storage path to reason about.
- The Storage seam is a strategy interface, so adding Postgres later is additive,
  not a rewrite.
