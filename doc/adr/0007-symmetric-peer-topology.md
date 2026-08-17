# ADR-0007 — Symmetric peer topology (the organizing principle)

**Status:** proposed
**Date:** 2026-08-16
**Superseded by:** —

## Context
The system historically lived through a server/client split (legacy collab +
WebSocket, then a separate `portalis-nexus` crate), and the forward product goal
(online *and* offline sharing, two-node Jetson+Atlas) kept pulling toward duplicated
server/client logic. The single most important decision reframes the whole system:
**there is one Nexus core; "server" and "client" are runtime roles, not separate
codebases.**

## Decision
**One Nexus core.** Server/client is a **runtime role**; any node can be a
coordination server, a participating client, or both at once. `apps/server` and
`libbackend` become **thin role-enabling shells** over the identical core.
Self-hosting is a **capability, not a separate product**.

## Why (pattern)
**Actor / symmetric-peer (role as runtime state, not code branch).** Treating
server/client as a role enabled at runtime — not as two code paths — deletes the
idea of duplicated server/client logic entirely, makes self-host/federation fall
out for free, and makes offline peer-to-peer natural (supports ADR-0003's offline
strategy). It is simultaneously the most powerful *and* the leanest stance: one
core, zero duplicated role logic.

## Consequences
- One core; `apps/server` and `libbackend` are thin shells switching on roles.
- BE-to-BE testing = two cores talking with no shell in between.
- Self-hosting and federation are capabilities of the one core, not separate builds.
- Open detail to resolve during design: **role negotiation** — how a node
  advertises "I can also coordinate."
