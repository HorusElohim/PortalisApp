# ADR-0018 — Treat generated bindings, security, coverage, and artifacts as release inputs

**Status:** accepted
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

- [x] FRB 2.13.0 regeneration produces no tracked diff. The frontend gate runs
      `tool/frb_build.sh --codegen-only --force-frb --ai` and fails on any
      generated-file drift. The local gate passed with the pinned 2.13.0
      generator; changing a tracked generated output is the failure condition.
- [x] Advisory/license/source policy runs in CI with explicit policy. `deny.toml`
      now enables advisories, licenses, sources, and duplicate-version policy;
      `tests/nexus.sh` runs `cargo audit` and
      `cargo deny check advisories bans licenses sources`. Local result:
      advisories, bans, licenses, and sources all passed.
- [x] Coverage remains scoped to production code. `tests/nexus.sh` invokes
      `rust/backend/scripts/coverage.sh`; the local gate passed at the existing
      enforced 95% function/region threshold with platform-only exclusions
      reported by the script.
- [x] Android release CI fails without signing material and verifies the signer
      when material is present. `build-android/action.yml` requires the
      keystore/password, fails before building when either is empty, and compares
      the APK signer SHA-256 certificate with the configured keystore certificate.
      Debug builds remain available locally through `flutter build apk --debug`.
- [x] Windows/macOS metadata contains the canonical Portalis publisher.
      `windows/runner/Runner.rc` and
      `macos/Runner/Configs/AppInfo.xcconfig` no longer contain the template
      `com.example` publisher/copyright values.
- [x] Android, Linux, macOS, iOS, and Windows release jobs are present in
      `.github/workflows/pipeline.yml` and remain release-gated by the backend
      and frontend jobs. Android debug and Linux-host backend checks passed
      locally; Apple/Windows hosted release execution requires their respective
      CI SDKs and is not claimed as a local result.
- [x] `.hermes/` is ignored at repository root, so local Hermes planning/session
      state does not dirty a normal worktree.

Verification performed for this implementation: frontend gate passed
(`flutter analyze`, 187 Flutter tests, FRB drift check); Rust formatting,
clippy, cargo audit/deny policy, and 283 backend tests passed; Android debug
APK builds for arm64-v8a, x86_64, and armeabi-v7a. Hosted Apple/Windows release
jobs are configured in CI but were not executable on this Linux host.
