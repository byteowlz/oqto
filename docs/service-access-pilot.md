# Design Studio service-access pilot

**Review artifact, not live completion.** Issue `oqto-j0vk` remains in progress.
This artifact authorizes no data, grants or deployment beyond the explicit
first-two-phase approval; phase 3 and browser/UI work are not approved here.

## Pinned sources and scope

- Oqto implementation/test revision: `9e8d898d337052d8a1377bea0eba5ce564eaaeee`.
- Independent peer-credential fix: `bff14af7`; base: `origin/main` at `0c93b3b1`.
- Coordination source: Governor `docs/briefs/tailnet-service-access-pilot.md`,
  approved 2026-10-11; [decision](adr/0057-bounded-design-studio-service-access-pilot.md).
- Studio owner reported unified source `c03ec6d`/`092ca1e`; native auth inspected
  read-only at `092ca1e901fa180f36209f27e7865141d7c40ea8`,
  `studio/feedback/server.py` and `studio/feedback/store.py`.
- Owner reports canonical Studio on **4782**, with **4783 only a compatibility
  redirect**. This agent did not probe, restart, deploy or edit either endpoint.
  No previews, client gallery, separate Studio checkout or UI were added.
- ADR-0029/0039/0046 remain proposed: no MFA, complete Operator API, mediated
  placement endpoint or per-agent isolation is claimed by this pilot.

The optional slice supports exactly `design-studio`, one configured existing
shared Workspace record, service resource `catalog`, and independent workload
identity `design-review`. `catalog` identifies this service resource, **not a
path-level/content-level permission or an App Action grant**. No general service
registry or caller-selectable upstream is implemented.

## Authority and boundaries

The authoritative state is Oqto's existing SQLite database: Account activity and
roles in `users`, Workspace membership/ownership in `shared_workspaces` and
`shared_workspace_members`, plus new `service_access_grants` and
`service_access_audit` tables. No Pi Session history, runner credentials, App
Instance grants, browser storage or ad-hoc production files are used as grants.

Human API access uses the existing `AuthState` signature verification for
Oqto-issued Account JWTs, then current active human Account and membership facts.
No dev bypass, copied service-role login, cookie, forwarding claim or coarse
admin membership bypass is accepted. The inherited human JWT format is
`workspace-backend`, audience-less, with existing expiry; the adapter additionally
checks exact expiry and not-before. It is **not** a new audience-bound browser
session, MFA adapter or server-side logout/revocation system. Removing membership
or deactivating the Account denies subsequent checks.

The workload has no human Account/login or general `CurrentUser` privileges.
Issuance generates 256 random bits as `oqto_sa_` plus 64 hex characters; only its
SHA-256 hash is stored. Its grant pins Workspace/resource/service/audience,
expiry (maximum 3600 seconds), owner baseline, and revocation. The acting issuer
is `local-admin`; `issued_by` records the current owner Account whose continuing
active membership and ownership bound the delegation. Ownership or membership
loss denies subsequent workload requests. One live workload grant is allowed
**globally**, including across concurrent issuance; scopes are never unioned.

Grant/revoke are available only on Oqto's existing owner-only admin Unix socket,
under its root/backend-uid policy. Remote human/admin JWTs cannot mutate grants.
This is existing local OS authority, not future remote Operator administration
or human MFA. [Peer-credential proof and limits](agents/local-admin-peer-credentials.md).

## Configuration and semantic operations

Disabled by default: absent configuration means no pilot routes. A reviewed
opt-in configuration uses the existing Oqto `config.toml` loader, for example:

```toml
[service_access_pilot]
audience = "https://design-studio.example.test"
workspace_id = "an-existing-workspace-id"
```

Only a canonical HTTPS origin is accepted. Unknown fields/unsupported syntax
and dev-mode auth fail closed. This file does not configure an upstream, create
a Workspace, mint a grant, change networking or deliver credentials to a runner.
No configuration on a real installation was changed for this artifact.

API version: `/api/service-access/v1`. Input schemas:
[`schemas/service-access-pilot.schema.json`](schemas/service-access-pilot.schema.json).
All successful responses and typed errors use JSON and `Cache-Control: no-store`.
The endpoint table describes supported operations, not authorization to use them.

| Operation | Authority/input | Response/effect |
|---|---|---|
| `GET /check` | Explicit edge credential plus exact scope query | `{schema,allowed:true,event_id}` or typed denial; durable audit required before allow |
| `GET /discovery` | Same scope/credential and decision | Array of `{service_id,resource_id,audience}` or empty array; no upstream addresses |
| `GET /grants` | Current human member, or trusted local admin | Grant metadata array, never credential/hash; read-only |
| `POST /grants` | Trusted local admin; reviewed request below | `{grant,credential}` shown once; transactional singleton/owner validation and audit |
| `POST /grants/{id}/revoke` | Trusted local admin | `{schema,revoked:true,grant_id}`; idempotent, transactional audit; unknown ID 404 |

Scope query fields: `service_id`, `workspace_id`, `resource_id`, `audience`.
Authentication is **only** `X-Oqto-Service-Authorization: Bearer <credential>`:
existing JWT for the authenticated human API path, or the separately issued
opaque workload credential. Never put credentials in URLs, process arguments,
logs, screenshots or shared browser cookies. Cookie/native Studio Authorization
cannot authenticate this edge. Production TLS/server identity remains required.

Grant request (synthetic illustration; expiry must be a reviewed future Unix
second, strictly greater than server time and at most server time + 3600):

```json
{
  "id": "reviewed-unique-grant-id",
  "workload_id": "design-review",
  "scope": {
    "service_id": "design-studio",
    "workspace_id": "an-existing-workspace-id",
    "resource_id": "catalog",
    "audience": "https://design-studio.example.test"
  },
  "expires_at": 1,
  "owner_account_id": "expected-current-owner-account"
}
```

CLI uses the same API, not direct database/file writes:

```sh
scripts/dev/service-access-pilot.sh --help
scripts/dev/service-access-pilot.sh /explicit/admin.sock inspect
# Only after actual parent review; --apply is intent, not a grant:
umask 077
scripts/dev/service-access-pilot.sh /explicit/admin.sock grant --apply \
  < reviewed-grant.json > private-issued-grant.json
scripts/dev/service-access-pilot.sh /explicit/admin.sock revoke reviewed-unique-grant-id --apply
```

Machine stdout is JSON; failures have nonzero exits. HTTP/transport CLI failures
return `cli_request_failed`; API reasons include `scope_denied`, `human_denied`,
`credential_invalid`, `grant_expired`, `grant_revoked`, `issuer_authority_lost`,
`local_admin_required`, `invalid_grant`, `grant_conflict_or_owner_changed`,
`credential_overlap`, `ambiguous_credentials` and `authority_unavailable`.
Read-only inspection reveals no credential. Issuance is **not retry-idempotent**:
a lost/timeout response may have committed, and the secret cannot be retrieved.
Inspect by the caller-chosen ID, revoke if needed, and remint under a fresh ID.
Restart/reconnect reloads durable facts; revoked IDs cannot be resurrected.

Audit records contain UUID, time, operation, bounded actor/grant IDs, decision
and stable reason, never credentials, native tokens, cookies, bodies or private
content. Successful grant/revoke audit commits with the mutation; access audit
failure prevents allow. This is a bounded pilot security log, not the full
Operator audit/retention/export/tamper-evidence authority from ADR-0029.

## Caddy contract and credentials

[`examples/service-access-pilot.Caddyfile`](examples/service-access-pilot.Caddyfile)
is an executed **synthetic HTTP template**, not deployed production config.
Parent must merge phase-1 real-peer source gates, trusted HTTPS, actual public
host/audience, Workspace and owner-reviewed upstream binding. Catch every Host;
unknown services/Immich deny. The template binds ephemeral loopback and proxies
to a fixture Unix socket; it is not a new production daemon.

Client identity claims do not authenticate. Caddy overwrites the original
method for the gate, strips identity/API-key headers, consumes/removes all
`X-Oqto-*` headers before Studio, and drops **all Cookie**, including mixed
Studio/Oqto cookies. Only Studio's pinned `Bearer` URL-safe 43-character syntax
is retained on `/api/feedback` or `/api/feedback/*`; other Authorization is
omitted. Credential query parameters are rejected. Headers/bodies are not logged.

Syntax is not credential provenance or native authentication. Proof generates
actual Oqto JWT/API-key/workload credentials and checks their disjoint formats,
checks omission/mixed-header denial, rejects a native-looking edge credential,
and simulates a known future/imported native-looking workload hash: the gate
denies that collision rather than forwarding it. Unknown external 43-character
strings remain Studio's native authentication responsibility. No universal
credential-provenance guarantee is claimed.

Studio alone verifies its actual feedback token and action permissions. Access
is **not a claim that the legacy UI or feedback API is read-only**. This pilot
admits GET/HEAD only; write methods and declared SSE/WebSocket upgrades are
unsupported. That method restriction is not a semantic action authorizer.

## Executed synthetic proof

Platform: macOS arm64; Caddy `v2.11.7`. All fixtures use a private temporary
SQLite database, synthetic Accounts/credentials, ephemeral loopback HTTP,
a fixture Unix upstream and child processes torn down by the tests. No live
Studio, ACL, privileged daemon, root lane or real grant/credential was used.

```sh
just service-access-pilot-proof
cd backend
cargo test -p oqto --lib auth::
cargo test -p oqto --lib auth::local_admin
cargo check -p oqto
cargo fmt --check
cargo clippy -p oqto --lib --bin oqto --test service_access_pilot -- -D warnings
cd ..
just lint-rust-ai-guardrails
bash -n scripts/dev/service-access-pilot.sh
```

Observed outcomes at the pinned source:

- Standard pilot suite: **5 passed, 0 failed, 1 explicitly ignored** (Caddy
  prerequisite). The second command in `just service-access-pilot-proof` runs
  that test: **1 passed, 0 failed**. Six pilot tests therefore executed.
- Auth unit selection: **39 passed, 0 failed, 1 ignored** (existing manual hash
  generation). Peer selection: **3 passed, 0 failed**; one real same-uid probe,
  two synthetic policy tests covering errors/unavailable/different uid.
- Cargo check, workspace fmt check and targeted clippy `-D warnings`: passed,
  warning-free. Changed-production Rust guardrails: passed, zero findings.
- Real public/admin Oqto routers prove disabled routes, permitted human API
  access, scoped workload access, public coarse-admin mutation denial and
  inability to use pilot credentials through general Account authentication.
- Lifecycle proof covers unrelated same-machine credential, Immich/service,
  Workspace/resource/audience mismatch, expired/future/forged human claims,
  workload expiry, ownership/membership loss, filtered discovery, revocation,
  concurrent singleton issuance, durable database reopen and authority failure.
- CLI grant/inspect/revoke is compared with authenticated human API inspection
  and HTTP authorization against the same store.
- Real Caddy proof covers allowed human/workload HTTP, spoofed identities,
  native credential filtering/collision, unknown/Immich host, unsupported
  methods/SSE/upgrades, URL credential rejection, revoke and auth outage.

Structural review commands were also run:

```sh
ripwire . --quality-delta --legend=compact
ripwire . --pr-context --legend=compact --max-tokens=4000
ripwire . --edit-check=backend/crates/oqto/src/api/routes.rs:create_router_with_config_and_auth --legend=compact
ripwire . --edit-check=backend/crates/oqto/src/main.rs:run_admin_socket_server --legend=compact
ripwire . --test-gate --legend=compact
```

These are **not all green**. Pre-source-commit quality delta returned 2 with
38 gating normalized clone/reused-helper findings. Reviewed examples compare
unrelated SQL `.bind` chains, record builders and child-process Drop cleanup;
sharing those domain stores would violate their different authorities. No
blanket acknowledgements/suppressions were added. Sequential fixture verbosity,
small composition growth and macro/callback dead-code reports remain explicit
review trade-offs. Contract edit checks report unchanged signatures (exit 0).
PR context is evidence, not certification. Test-gate returned 4 for required
tests/name-based untested radius; actual HTTP/router tests above passed, but
broad unrelated subsystem coverage is not claimed. Re-run against the peer-fix
parent (`bff14af7`) to inspect the implementation delta after checkout.

## Revocation bounds and outstanding live gates

- **New checks after committed revoke have no authorization-cache delay:** they
  consult the store and deny. Already-running checks/responses can finish.
  No wall-clock production latency SLA or teardown bound is promised.
- Declared SSE/upgrades are rejected. Long-lived/disguised streaming GETs and
  already authorized responses are **not cancelled by this gate**. Active-stream
  revocation is unsupported and must not be advertised as implemented.
- Fixture Studio exposes only a Unix upstream, not a TCP port. The test also
  proves the same-uid administrative direct-socket path remains reachable.
  It does **not** prove a restricted placement cannot reach that socket.
- **Live direct 4782 currently bypasses the gateway.** Keep existing oqto-dev
  access unchanged while other agents depend on it. Parent/Studio owner must
  coordinate upstream/ACL migration only after HTTPS is actually proven from
  oqto-dev. This artifact cannot claim live pilot enforcement before that proof.
- Workload credential delivery, mediated endpoint attachment, namespace/uid
  separation and local/bwrap/container/remote placement matrix are unverified.
  Independent credential denial on the same machine is not OS process isolation;
  no per-session/per-agent separation is claimed. Same-uid secret theft or an
  accessible direct upstream would defeat the intended deployment boundary.
- **Browser edge login is unsupported.** Human proof is backend/API/CLI only;
  no host-only browser session handoff, wildcard cookie, login UI or completed
  browser-human phase 2 is claimed. Never share human login credentials.
- Live hostname/TLS/Caddy source-gate binding, canonical Studio HTTP proof,
  auth-service topology, actual native feedback token compatibility, client
  gallery exclusion and restricted-caller upstream denial need parent/owner
  review and live evidence. Cross-uid/root platform and Linux execution probes
  remain unverified; the same-uid macOS probe does not satisfy them.

## Proportional agent-readiness gates

| Gate | Status and evidence |
|---|---|
| discoverability | pass for this API/CLI scope: versioned docs/schema/help and fixture CLI task |
| structured_contract | pass for supported operations: JSON input/output, stable typed invalid-input errors, nonzero CLI errors |
| shared_substrate | unverified overall: CLI/authenticated human API parity passes; no browser human presentation |
| authority_and_safety | unverified overall: synthetic auth/negative/intent tests pass; live placement/upstream/TLS authority remains unproved |
| concurrency_and_recovery | pass for bounded API: singleton race, duplicate-ID rejection, idempotent revoke, durable reopen; uncertain issuance recovery documented |
| portable_state | unverified overall: grant metadata JSON and SQLite restart pass; credential import/export/production restore intentionally unsupported |
| reproducibility | pass for synthetic scope only: `just service-access-pilot-proof`; live/native/Linux claims excluded |
| human_accessibility | not_applicable to this backend/CLI-only pilot; browser UX is unsupported, not certified |

The issue must remain open pending parent-reviewed live proof and the explicitly
unimplemented browser/placement/stream capabilities where required.
