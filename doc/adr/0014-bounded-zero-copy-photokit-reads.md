# ADR-0014 — Bound zero-copy PhotoKit reads

**Status:** proposed
**Date:** 2026-09-01
**Superseded by:** —

## Context

Portalis must seed Photos-library media from native references without copying
or cloning source files. When PhotoKit exposes a direct URL, Rust can use bounded
random-access reads. The fallback API delivers a sequential stream; the current
implementation starts that stream at byte zero for every requested range,
discards preceding bytes, waits for the entire request even after the range is
filled, and waits forever on semaphores. Large or iCloud-backed media can incur
quadratic work or permanently block a torrent storage worker.

## Decision

Keep PhotoKit as the zero-copy authority while making reads bounded.

- Prefer and reuse a direct read-only file handle when PhotoKit exposes a URL.
- For streaming-only assets, use a sequential reader suitable for hashing and
  consecutive torrent-piece reads instead of restarting from byte zero for each
  range.
- Cancel the PhotoKit request as soon as the required range is complete.
- Apply finite permission, metadata, read, and completion timeouts.
- Propagate timeout, cancellation, short-read, and access-revoked errors to Rust
  with actionable context.
- Coordinate concurrent reads without materializing the original asset in the
  Portalis container.
- Never create a cache-file clone as a fallback.

## Consequences

- Zero-copy Photos sources remain the only publication source.
- Large and iCloud-backed assets avoid repeated prefix reads.
- A stalled PhotoKit callback cannot block a worker forever.
- The native reader gains explicit lifecycle and cancellation tests.

## Acceptance verification

- [ ] Direct-URL reads remain exact and zero-copy.
- [ ] Sequential fallback reads each source byte at most once per stream.
- [ ] A completed range cancels the remaining PhotoKit request.
- [ ] Every semaphore wait is finite and timeout-tested.
- [ ] Concurrent and out-of-order reads are correct or fail explicitly.
- [ ] No source copy/cache clone is created.
- [ ] iOS Rust build and native tests pass.
