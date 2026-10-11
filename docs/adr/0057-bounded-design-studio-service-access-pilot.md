# Bounded Design Studio service-access pilot

## Status

Review implementation for `oqto-j0vk`, under the approved first-two-phase
service-access brief (2026-10-11). **Not a production rollout or a general
Operator authentication platform.** ADR-0029, ADR-0039 and ADR-0046 remain
proposed contracts, not shipped guarantees.

## Decision

Keep one optional service-access slice inside Oqto's existing auth boundary.
Reuse verified human Account JWTs, active Account facts and current Workspace
membership. App Instance grants and runner transport credentials authorize
other domains and must not be repurposed as external service-access grants.

Bound the slice to `design-studio`, one configured existing shared Workspace,
one `catalog` service resource and one independent `design-review` workload.
An opaque random credential references a durable grant with exact audience,
expiry and revocation. It never becomes a `CurrentUser`, borrows a human login,
or acquires the existing broad service role. One active workload grant is
allowed globally; different grants are never unioned.

Inspect, issuance, revocation, filtered discovery and Caddy checks use that same
SQLite authority. Mutations use only the existing protected local admin socket;
a remote coarse-admin JWT cannot issue or revoke. The current Workspace owner
is an issuance baseline and ongoing delegation ceiling, not a claimed actor;
the audit identifies issuance as `local-admin`. No remote MFA/Operator API
administration is claimed.

Use a separate edge credential header, consumed before Studio. Drop all
cookies. Only the pinned native Studio feedback bearer syntax may pass through
on feedback paths; deny known Oqto credential collisions. Syntax is not native
authentication: Studio still owns its native login/action decision.

## Consequences

No GUI state or redesign, upstream URL registry, new daemon, production grants,
ACL changes, cookie-domain sharing or automatic credential delivery. Browser
edge login and placement isolation are not implemented. Scope is no finer than
the credential's eventual isolation boundary; the fixture cannot prove that
boundary. Already authorized HTTP responses have no teardown bound.

The pilot remains incomplete until coordinated live Studio, upstream-bypass,
placement and source-gate proof passes. Details, semantic contracts, evidence
and limitations: [service-access-pilot](../service-access-pilot.md).
