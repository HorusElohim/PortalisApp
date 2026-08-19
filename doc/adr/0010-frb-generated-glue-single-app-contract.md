# ADR-0010 — Generated FRB glue behind one app-facing contract

**Status:** proposed  
**Date:** 2026-08-19  
**Supersedes:** [ADR-0001](0001-single-api-seam.md)

## Context

ADR-0001 correctly identified that the app must not choose between multiple
backend contracts. Its wording also described generated `api.rs` as a path to
delete. In the current Flutter Rust Bridge integration, `api.rs` is generated
transport glue required to connect Dart to Rust; it is not itself a second
product API. The deliberate Rust-facing surface is `portalis_api.rs`, and the
Dart application consumes it through `AppRepository` and the generated bridge
DTOs.

The important distinction is therefore between the **generated mechanism** and
the **designed contract**. Deleting generated glue would break the bridge;
letting application code depend on arbitrary generated functions would recreate
the original problem.

## Decision

There is exactly one app-facing contract:

- Rust owns `AppSnapshot` and the closed command model in `portalis_api.rs`.
- Flutter reads snapshots and sends commands through the minimal
  `AppRepository` seam.
- `api.rs` and generated Dart bridge files remain implementation artifacts of
  Flutter Rust Bridge. They are regenerated, not hand-edited, and are not a
  second application API.
- New app capabilities enter through the deliberate snapshot/command contract;
  internal Rust functions are not exposed merely because code generation can
  see them.

## Why (pattern)

**Facade + Command pattern with generated adapter glue.** The facade is the
small app contract; the command model makes writes explicit and auditable; FRB
generated files are adapters underneath it. This preserves one source of truth
without confusing generated plumbing with a public design surface.

## Consequences

- The bridge can be regenerated without changing the application architecture.
- Generated files must never be manually patched to add product behavior.
- Flutter tests fake `AppRepository`, not FFI internals.
- A schema change updates the Rust contract, regenerated bindings, Dart seam,
  and focused round-trip tests together.
- The old two-surface problem is resolved by ownership and dependency direction,
  not by deleting required generated transport code.

## Non-goals

This ADR does not redesign Flutter state management, remove the repository seam,
or define a different serialization protocol.

**References:** `portalis/rust/backend/src/portalis_api.rs`, generated
`portalis/rust/backend/src/api.rs`,
`portalis/lib/nexus/data/app_repository.dart`, and
`portalis/lib/nexus/domain/app_state.dart`.

## Supersession note

ADR-0001's single-seam goal remains valid. This record supersedes only its
incorrect implication that required generated FRB glue is itself an obsolete
app-facing API path.
