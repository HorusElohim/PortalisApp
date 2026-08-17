# ADR-0004 — Naming discipline: no same-name across levels

**Status:** proposed
**Date:** 2026-08-16
**Superseded by:** —

## Context
The tree carries same-name collisions that break grep-driven navigation (how both
the owner and AI agents move through the code): `src/nexus.rs` vs
`src/core/nexus.rs`, `src/torrent.rs` vs `src/core/torrents.rs`,
`friends.rs` vs `friendship.rs`. A `grep nexus.rs` returns two unrelated files;
`src/torrent.rs` is a self-labeled debug tool yet force-compiled and 6 of its
functions are bridged to Dart.

## Decision
**Do not do a standalone rename pass.** Kill the same-name collisions as the
redesign rewrites/merges those files, and enforce a **no-same-name-across-levels**
rule going forward as a standing constraint.

## Why (pattern)
**Single source of truth / one name per concept (DRY applied to names).** A name is
a navigation token; two files sharing a name at different levels make that token
ambiguous. Polishing files headed for a rewrite wastes effort, so the collisions
are resolved *by* the rewrite, not before it — but the rule itself is a permanent
constraint, not a one-time cleanup.

## Consequences
- New modules must not reuse an existing name at a different tree level.
- Existing collisions are retired as the affected files are rewritten/merged under
  ADR-0001 / ADR-0003 / ADR-0007, not in a separate pass.
- grep-driven navigation (human and agent) regains unambiguous results.
