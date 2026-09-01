# ADR-0012 — Restrict exact-source descriptor fetching

**Status:** proposed
**Date:** 2026-09-01
**Superseded by:** —

## Context

Portalis accepts untrusted magnet links. A magnet may contain an `xs` exact-source
URL naming an immutable `.torrent` descriptor. The current Rust path accepts both
HTTP and HTTPS and passes the URL to an unrestricted `reqwest::get` before it
verifies the returned info hash. A crafted link can therefore make Portalis
contact loopback, LAN, link-local, or metadata services. Redirects and response
size are not constrained independently.

Exact-source retrieval is only an optional metadata hint. It must never fetch
media payload bytes, replace DHT/trackers/direct peers, or weaken the existing
HTTPS path.

## Decision

Rust owns one restricted exact-source descriptor client.

- Accept HTTPS descriptor URLs. Plain HTTP is rejected.
- Reject embedded credentials and non-default URL forms that cannot be safely
  validated.
- Resolve the destination and reject unspecified, loopback, multicast,
  link-local, documentation, carrier-grade NAT, and private addresses for both
  IPv4 and IPv6.
- Disable automatic redirects. If redirects are supported later, every target
  must pass the same scheme, credential, DNS, and address checks before a new
  request is made.
- Apply finite connect, header, body, and total timeouts.
- Enforce a small descriptor body limit before buffering it.
- Decode the descriptor and require its BTv1 info hash to equal the magnet's
  expected hash before admission.
- Treat every refusal, timeout, transport error, oversized response, decode
  failure, and hash mismatch as an unavailable hint, then continue through DHT,
  trackers, and direct peer hints.
- Never fetch torrent payload bytes through this client.

## Consequences

- Untrusted magnets cannot use Portalis as an SSRF client into local networks.
- HTTPS exact-source metadata remains supported.
- Private, trackerless Portalis collections continue to bootstrap through their
  direct peer hints when no acceptable descriptor is available.
- Request policy and body limits are testable without a process-global client.

## Acceptance verification

Status remains `proposed` until all are cited by passing tests:

- [ ] HTTPS public descriptor with matching info hash is accepted.
- [ ] HTTP, credentials, loopback, private, link-local, multicast, and
      unspecified targets are rejected before a request is sent.
- [ ] Redirects cannot bypass destination validation.
- [ ] Oversized and slow bodies are bounded.
- [ ] Descriptor hash mismatch is rejected.
- [ ] Every exact-source failure falls back to the ordinary magnet path.
- [ ] No payload/media request is introduced.
