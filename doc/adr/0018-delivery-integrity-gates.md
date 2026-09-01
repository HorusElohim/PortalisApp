# ADR-0018 — Treat generated bindings, security, coverage, and artifacts as release inputs

**Status:** proposed
**Date:** 2026-09-01
**Superseded by:** —

## Context

A release can be green while carrying stale FRB output, unscanned Rust
advisories, broad coverage exclusions, a debug-signed Android artifact, or
placeholder desktop publisher metadata. Local planning files also keep the tree
permanently dirty. These are delivery-contract failures rather than product
features, so they need one explicit gate policy.

## Decision

CI and release tooling verify every generated/security/artifact input.

- Pin `flutter_rust_bridge_codegen` to the same version as Rust and Dart FRB.
- Force regeneration in CI and fail on tracked generated-file drift.
- Add Rust advisory/license/source policy checks with reviewed exceptions.
- Replace broad top-level Nexus coverage exclusions with explicit justified
  platform-only exclusions; remove stale migration commentary.
- Require PR checks for protected integration branches; direct development
  pushes are not considered release evidence.
- Distribution Android builds require a stable release/upload key and verify the
  signer certificate. Debug-signed sideload artifacts are named separately.
- Replace Windows/macOS template publisher and copyright metadata.
- Keep HTTPS, UPnP, native source references, and supported platform builds in
  the release matrix.
- Ignore local `.hermes/` planning state unless the repository deliberately
  chooses to track a plan.

## Consequences

- Generated bridge drift and vulnerable dependencies fail before release.
- Coverage percentages describe the intended production scope honestly.
- A release-named Android artifact is genuinely release-signed.
- Worktrees can be clean and automation-friendly.

## Acceptance verification

- [ ] FRB 2.13.0 regeneration produces no diff on a clean tree and a stale
      fixture fails the gate.
- [ ] Advisory/license/source policy runs in CI with no unreviewed exceptions.
- [ ] Coverage includes critical zero-copy, TLS, bootstrap, and torrent adapter
      logic or documents narrow platform exclusions.
- [ ] Android release CI fails without signing material and verifies the signer
      when material is present.
- [ ] Windows/macOS metadata contains the canonical Portalis publisher.
- [ ] Android, Linux, macOS, iOS, and Windows release jobs pass.
- [ ] `.hermes/` no longer dirties a normal local worktree.
