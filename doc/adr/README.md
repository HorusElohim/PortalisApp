# Portalis — Architecture Decision Records

Frozen, append-only decision records. The **current-state map is the code**; this
index + the non-superseded ADRs is the decision history. ADRs are superseded, never
edited in place. Status: `proposed` → (owner validation at merge) → `accepted` →
`superseded` (points at its successor).

See [ADR-0005](0005-decision-records-adrs.md) for why this repo uses ADRs + index
instead of a living SPEC.

## Index

| # | Title | Status | Superseded by |
|---|-------|--------|---------------|
| [0001](0001-single-api-seam.md) | Single API seam: AppSnapshot + Command | proposed | — |
| [0002](0002-server-storage-redb-postgres.md) | Server storage: redb now, Postgres scale target, Mongo + duplicate in-memory engine deleted | proposed | — |
| [0003](0003-transport-substrate-dual-engine.md) | Transport: Substrate as a Strategy with two real engines | proposed | — |
| [0004](0004-naming-discipline.md) | Naming discipline: no same-name across levels | proposed | — |
| [0005](0005-decision-records-adrs.md) | Decision records: ADRs only + index, no living SPEC | proposed | — |
| [0006](0006-frontend-thin-seam.md) | Frontend shape: one thin seam layer | proposed | — |
| [0007](0007-symmetric-peer-topology.md) | Symmetric peer topology (the organizing principle) | proposed | — |

> **Note:** ADRs 0001–0007 record decisions already agreed in the 2026-08-16
> decision session (see `.hermes/plans/`). They are `proposed` pending owner
> validation at PR merge; the corresponding code changes (delete the second API
> surface, delete the Mongo backend, redesign Substrate, etc.) are **not yet in
> the tree** and will be sequenced per the plan.
