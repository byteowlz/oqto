# Oqto Roadmap

> Last updated: 2026-02-22
>
> This document is the living roadmap for Oqto's architecture evolution. It is
> organized by strategic pillars and prioritized within each pillar.
>
> For day-to-day task tracking, use `trx ready` to see unblocked issues.

---

## Overview

Oqto is a self-hosted AI agent workspace platform. This roadmap balances three
strategic pillars:

1. **Security** - Defense in depth, zero-trust runner verification, secure defaults
2. **Stability** - Graceful degradation, self-healing, observability
3. **Scalability** - Horizontal scaling, protocol evolution, enterprise readiness

---

## Pillar 1: Security

### P0: Critical

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-e067] | Runner mTLS authentication and attestation | open | Prevent runner impersonation with cryptographic verification |
| [oqto-qq9y] | Security audit sudoers configuration | open | Review multi-user mode privilege escalation risks |

### P1: High

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-cxxr] | Defense-in-depth sandboxing (seccomp-bpf) | open | Fallback beyond bwrap for sandbox isolation |
| [octo-1ddx] | Move global sandbox config to read-only path | open | Prevent runtime sandbox policy modification |

### P2: Medium

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| TBD | eavs virtual key rotation automation | - | Automatic key rotation without session interruption |
| TBD | Structured security audit logging | - | SIEM-compatible audit trail for compliance |

---

## Pillar 2: Stability

### P0: Critical

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-29e1] | hstry gRPC HA with local spool fallback | open | Eliminate hstry as single point of failure |
| [oqto-q4ae] | setup.sh must guarantee hstry is running | open | Bootstrap dependency orchestration |

### P1: High

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-mvdv] | Streaming reliability (backpressure, resync) | open | Robust event streaming under load |
| [oqto-fezg] | Circuit breaker for WebSocket connections | open | Prevent cascade failures |
| [oqto-q3qf] | Respawn dead runners before connecting | open | Self-healing runner lifecycle |

### P2: Medium

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-y05n] | Browser-side STT via Moonshine WASM | open | Reduce server-side dependency for voice |
| TBD | Session state persistence for crash recovery | - | Resume sessions after runner restart |

---

## Pillar 3: Scalability

### P0: Critical

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| TBD | Stateless backend validation | open | Verify backend can scale horizontally |

### P1: High

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-35fg] | PostgreSQL backend for hstry | open | Beyond SQLite limits for enterprise scale |
| [oqto-8f14] | Protocol versioning | open | Safe canonical protocol evolution |
| [oqto-t2bf] | Multi-Runner & Workspace Sharing | open | Distributed compute architecture |
| [oqto-pdb4] | Shared workspaces with multi-location runners | open | Cross-runner collaboration |

### P2: Medium

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [oqto-wdkj] | Remote runner bootstrap over SSH | open | Self-service runner provisioning |
| [oqto-xxe2] | Per-workspace hstry/mmry stores | open | Data isolation and sharding |
| [octo-wg67] | @@ cross-agent routing in runner | open | Local delegation optimization |

---

## Pillar 4: Technical Debt & Foundation

### P1: High

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [octo-nqg8] | Eliminate all Rust warnings and unsafe code | open | Code quality and safety |
| [octo-y5r3] | Make hstry the sole history source | open | Single source of truth migration |
| [oqto-c6n3] | Route file operations through oqto-files | open | Unified file access layer |

### P2: Medium

| Issue | Title | Status | Description |
|-------|-------|--------|-------------|
| [octo-zjs8] | Add strict clippy lints | open | Automated code quality |
| [oqto-fr2f] | Admin UI for eavs providers | open | Operational tooling |

---

## Current Sprint Focus

**Sprint Theme: Stability & Reliability**

Active work:
- [oqto-29e1] hstry HA with local spool (blocks enterprise readiness)
- [oqto-mvdv] Streaming reliability improvements
- [octo-nqg8] Unsafe code elimination

Next up:
- [oqto-e067] Runner mTLS (security hardening)
- [oqto-8f14] Protocol versioning (enables safe iteration)

---

## Decision Log

### 2026-02-22: Prioritization Refresh

Following critical architecture review:
1. **Elevated** hstry HA to P0 - identified as true SPOF
2. **Elevated** runner mTLS to P0 - impersonation risk unaddressed
3. **Added** protocol versioning as P1 - required for safe evolution
4. **Added** PostgreSQL backend as P1 - enterprise scaling requirement

### 2026-02-15: Canonical Protocol Stabilization

Canonical protocol defined as DRAFT in docs/design/canonical-protocol.md.
Migration plan established for pi-specific → canonical translation.

---

## Contributing to the Roadmap

1. **New Issues**: Create via `trx create` with appropriate priority
2. **Priority Changes**: Document in Decision Log above
3. **Completion**: Update this file when issues close

## Links

- [Architecture Overview](./docs/design/canonical-protocol.md)
- [Issue Tracker](../../.trx/issues/)
- [Setup Guide](./SETUP.md)
