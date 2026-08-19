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
| [0003](0003-transport-substrate-dual-engine.md) | Transport: Substrate as a Strategy with two real engines | superseded | [0003.5](0003.5-single-bittorrent-substrate-discovery-strategies.md) |
| [0004](0004-naming-discipline.md) | Naming discipline: no same-name across levels | proposed | — |
| [0005](0005-decision-records-adrs.md) | Decision records: ADRs only + index, no living SPEC | proposed | — |
| [0006](0006-frontend-thin-seam.md) | Frontend shape: one thin seam layer | proposed | — |
| [0007](0007-symmetric-peer-topology.md) | Symmetric peer topology (the organizing principle) | proposed | — |
| [0003.5](0003.5-single-bittorrent-substrate-discovery-strategies.md) | One BitTorrent substrate, pluggable peer discovery | proposed | — |
| [0009](0009-qr-peer-bootstrap.md) | QR-first peer bootstrap for offline sharing | proposed | — |
| [0010](0010-frb-generated-glue-single-app-contract.md) | Generated FRB glue behind one app-facing contract | proposed | — |

> **Note:** ADRs record decisions agreed during design sessions and remain
> `proposed` pending owner validation at PR merge. ADR-0002 and the one-engine
> portion of ADR-0003.5 are already reflected in the tree; the remaining records
> describe direction and are sequenced with the code changes they motivate.
> A new ADR declares what it supersedes; older records remain unchanged as
> append-only history until the owner accepts the successor.
