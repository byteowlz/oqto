# Remote Operator API authentication, MFA, authorization, and enrollment

## Status

Proposed (2026-07-29). Extends ADR-0024's rule that transport identity is not
authorization. Related to ADR-0019 placement operability and the planned
`oqto-auth` extraction (`oqto-3ct7.7`). Tracked by `oqto-vt30`.

## Context

One `oqtoctl` installed on an operator's laptop should control many Oqto
deployments:

```text
oqtoctl --host archvm ws ls
oqtoctl --host octo-azure ws exec research -- cargo test
```

Today, many `oqtoctl` paths assume local files, a local admin Unix socket, or
local Podman. Oqto has application Accounts, coarse user/admin roles, Workspace
membership, frontend session authentication, a root-local admin socket, and
authenticated runner transports. It does **not** yet have a canonical remote
Operator API, OIDC integration, MFA, operator capability model, service
identities, enrollment lifecycle, or complete operator audit authority. Existing
`admin` checks must not be presented as that system.

The word `Host` already means the App capability-contract implementation in
Oqto's domain language. The ergonomic CLI flag remains `--host`, but its value
names a **Deployment Target**: a local CLI profile that identifies one Oqto
Control Plane. It never identifies a placement machine or implies SSH.

## Decision

### 1. One remote control boundary

Nearly every deployment-facing `oqtoctl` command calls a versioned HTTPS
Operator API on the selected Control Plane. The Control Plane authenticates and
authorizes the request, records the audit event, resolves the Workspace through
PlacementStore, and invokes the appropriate operator/runner interface where the
placement lives.

```text
laptop oqtoctl
  -> HTTPS Operator API
  -> authentication + authorization + audit
  -> PlacementStore / PlacementOperator / Runner
```

The laptop never opens the deployment's placement registry, invokes its Podman,
uses SSH as an application transport, or receives runner mTLS credentials.
Runner authentication remains an internal Control Plane-to-runner concern under
ADR-0024.

Only local CLI concerns remain local: Deployment Target management, login/logout,
credential storage, shell completion, local-file validation, and explicitly
selected laptop diagnostics.

### 2. Authentication and authorization are separate

Authenticators produce one typed `OperatorIdentity` plus an
`AuthenticationAssurance` record. The authorizer then evaluates that identity,
the requested capability, resource scope, assurance, and current policy.

```text
OIDC / WebAuthn / mTLS service credential
              -> OperatorIdentity + AuthenticationAssurance
              -> authorize(capability, resource, assurance)
```

An Operator Identity is either:

- a human Oqto Account linked to one or more external or WebAuthn credentials;
- an automation Service Identity with no interactive login.

It is not an OS Principal. Authentication method names, certificate subjects,
OIDC groups, and enrollment codes never become authorization roles by
implication.

### 3. Deployment Target and credential storage

`--host` selects a Deployment Target. Selection precedence is:

```text
--host > OQTO_HOST > current target in local config
```

A target profile stores public metadata only: alias, canonical HTTPS URL,
expected deployment identity, and authentication mode. Raw HTTPS URLs may be
used for ephemeral automation. Secrets, refresh credentials, and private keys
live in an OS credential store or an explicit secure credential provider such
as kyz; they are never written to the target configuration.

HTTPS server-certificate validation authenticates the Control Plane. Private or
Tailscale reachability is an additional network boundary, never authorization.
Plain HTTP is permitted only for an explicitly selected loopback development
target and can never carry production operator credentials.

### 4. Human authentication and MFA

Human remote administration requires MFA. The preferred factor is WebAuthn
(FIDO2 security key or passkey), either asserted by a trusted OIDC provider or
verified directly by Oqto during the pre-OIDC bootstrap phase. Password-only,
email-code-only, TOTP-only, client-certificate-only, and network-location-only
authentication do not satisfy the privileged-operator assurance policy.

An authenticator records evidence rather than a boolean `mfa=true`:

- methods used (`amr`, e.g. password, WebAuthn, hardware key);
- assurance class (`acr` or Oqto-mapped equivalent);
- authentication time;
- issuing authority and audience;
- credential/device identity where applicable.

OIDC is accepted only when issuer, signature, audience, nonce/state/PKCE, token
lifetime, and configured assurance claims validate. Oqto maps provider-specific
`acr`/`amr` values into its own assurance classes; an IdP's mere use of the word
"MFA" is not trusted without an administrator-configured mapping. Authorization
depends only on the internal assurance class, not directly on provider syntax.

Privileged or destructive actions require recent **step-up authentication**.
Initial policy classes are:

| Action class | Examples | Minimum assurance |
|---|---|---|
| Read | status, `ws ls/show`, health | authenticated operator; deployment may require MFA globally |
| Operate | logs, restart, repair | MFA |
| Execute | `ws exec`, terminal attachment, secret submission | recent MFA step-up |
| Administer | Accounts, capability grants, policy, images, Service Identities | recent phishing-resistant MFA step-up |
| Security critical | enrollment issuance, auth-policy change, break-glass reset | recent phishing-resistant MFA plus explicit confirmation |

The exact recency window is policy, with a conservative default. The server
checks it; a CLI flag cannot suppress step-up. A step-up result yields a new
short-lived token or elevated server session bound to the same identity,
deployment, audience, and client context.

### 5. Automation authentication

Automation uses a Service Identity authenticated by a scoped mTLS client
certificate or short-lived workload credential. Human refresh tokens and copied
browser sessions are forbidden in CI.

Service authentication is not described as MFA. Instead, Service Identities
must be narrowly capability- and resource-scoped, time-bounded where possible,
rotatable, revocable, non-interactive, and auditable. They cannot create human
enrollments, weaken authentication policy, or grant capabilities unless an
explicit security-administration policy says otherwise.

The initial mTLS implementation is an authentication adapter, not the
authorization model. Certificate fingerprints bind to an Operator/Service
Identity; possession of a valid certificate does not itself imply `admin`.

### 6. Authorization model

Every Operator API operation declares one typed capability and resource scope.
Initial capability families include:

```text
deployment.read
workspace.read
workspace.operate
workspace.exec
account.manage
policy.manage
release.manage
operator.manage
audit.read
```

Coarse roles are named capability bundles, not hard-coded route branches.
Workspace grants may narrow a capability to specific Workspace IDs. Deny and
revocation take precedence. An issuer cannot delegate a capability or scope it
does not hold. Reachability, Account ownership, Workspace membership, placement
kind, runner certificate validity, and an `admin` string are not substitutes
for this authorization decision.

The authorization decision returns allow/deny plus a stable reason code and the
policy/grant identities consulted. Failures are fail-closed and audit-visible
without leaking sensitive resource existence to unauthorized callers.

### 7. Enrollment and recovery

Installation creates one short-lived, single-use initial enrollment secret
available only through the deployment's local administrative channel. Consuming
it establishes the first human administrator and registers a phishing-resistant
MFA credential. The deployment does not become remotely administrable at high
privilege until that MFA binding succeeds.

An existing identity with `operator.manage` and recent phishing-resistant
step-up may create another enrollment invitation. Each invitation:

- has a cryptographically random secret shown once and stored only as a hash;
- is single-use, short-lived, rate-limited, revocable, and audience-bound to one
  deployment;
- fixes the maximum capability bundle and resource scope at issuance;
- cannot exceed the issuer's delegable authority;
- requires the enrolling human to prove/register its own identity and MFA;
- records issuance, attempted use, successful use, expiry, and revocation.

Enrollment is therefore re-triggerable by an authorized administrator without
reopening first-boot mode.

If all remote administrators are lost, an OS administrator on the Control Plane
host may create a local **break-glass enrollment** through the root-protected
admin Unix socket. It is never remotely callable without existing authorization.
It emits a high-severity durable audit event, invalidates prior recovery
material as policy requires, has a short expiry, and still requires the new
human administrator to bind MFA. Break-glass recovers control; it does not
silently disable MFA or erase existing identities.

### 8. Tokens, sessions, and revocation

Access tokens are short-lived, audience-bound to one deployment, and carry or
reference the Operator Identity, assurance context, authorization epoch, and
expiry. Refresh credentials rotate and are stored securely. Bearer tokens are
never placed in URLs, command history, logs, telemetry, or runner messages.

Mutations use idempotency keys. Streaming connections authenticate at attach and
are terminated or reauthorized when credentials expire, authorization changes,
or the identity is revoked. Logout revokes the local refresh/session credential;
security administrators can revoke an identity, credential, device, invitation,
or all sessions. Authorization-sensitive changes advance an epoch or otherwise
invalidate cached decisions promptly.

### 9. Audit authority

Operator auditing is a separate durable authority from oqto-log, which remains
Session-history authority. Every authentication, step-up, authorization denial,
enrollment event, credential change, capability grant/revoke, mutation, remote
execution, and break-glass action records:

- stable event and correlation IDs;
- deployment, Operator Identity, credential/session, and authentication
  assurance;
- capability, resource identity, decision, and reason;
- request origin metadata appropriate to privacy policy;
- result, timestamp, and idempotency key where applicable.

Secrets, bearer tokens, enrollment plaintext, command stdin, and secret payloads
are never audit fields. Remote execution records command metadata according to
policy, not secret-bearing content. Audit retention, export, tamper evidence,
and access are explicit deployment policy; audit readers require `audit.read`.

### 10. API and CLI behavior

The Operator API is versioned and advertises capabilities. Unknown versions,
unsupported authentication methods, insufficient assurance, and unsupported
commands fail with typed errors. Interactive streams (logs, ask, exec, terminal,
uploads) use bounded backpressure, cancellation, expiry handling, and reconnect
contracts rather than unbounded HTTP buffering.

`oqtoctl` may open a browser for OIDC/WebAuthn login and step-up. In headless
contexts it prints a device/verification flow only when the configured provider
supports a secure standard flow; it never asks the user to paste an access token
into a command argument. Destructive commands retain explicit confirmation and
non-interactive `--yes` semantics, but confirmation is not authentication or
authorization.

## Delivery sequence

1. Define `OperatorIdentity`, assurance classes, typed capabilities, resource
   scopes, authorization decisions, and audit events in the extracted auth
   boundary. Existing application `admin` remains a temporary adapter only.
2. Add the versioned HTTPS Operator API with server identity, capability
   negotiation, audit recording, and read-only `status`/`ws ls`.
3. Add local bootstrap and administrator-created enrollment with direct
   WebAuthn registration, revocation, and break-glass tests.
4. Add human login, short-lived sessions, MFA step-up, and capability bundles;
   migrate mutating command families only after their required capability and
   audit event are declared.
5. Add Service Identity mTLS for scoped automation.
6. Add OIDC Authorization Code + PKCE and provider assurance mapping to the same
   identity/assurance model; do not fork authorization by authenticator.
7. Move runtime operator execution behind the authenticated Control Plane API,
   then migrate the remaining deployment-facing `oqtoctl` commands.

A deployment may ship read-only remote operations before the full sequence, but
must not expose privileged mutation or execution under password-only, coarse
`admin`, unaudited, or transport-identity-only authorization.

## Verification contract

Executable tests must prove:

- wrong server identity, issuer, audience, signature, nonce/state, and expired
  credentials fail closed;
- authorization is independent of authenticator and enforces capability plus
  resource scope;
- low-assurance and stale-MFA sessions cannot execute, administer, enroll, or
  weaken policy; step-up cannot change identity or audience;
- an issuer cannot grant capabilities/scopes it lacks;
- enrollment is one-time, expires, hashes its secret, resists replay/races, is
  revocable, and requires MFA binding;
- break-glass requires the local root-protected channel and emits a durable
  high-severity event;
- credential, identity, grant, and session revocation terminate new requests and
  active streams within the declared bound;
- tokens, enrollment secrets, and secret-bearing inputs never appear in logs,
  audit events, URLs, process arguments, or runner frames;
- mutation idempotency and stream reconnect do not duplicate operations;
- LocalProcess, container, and remote placements receive identical authorized
  Workspace operations through the Control Plane and runner contracts;
- audit events correlate authentication, authorization, action, and result.

## Rejected alternatives

- **SSH as the normal oqtoctl transport:** couples the client to deployment
  topology and host accounts, bypasses the Operator API authorization/audit
  boundary, and does not extend to managed clusters cleanly.
- **Client certificate equals administrator:** confuses transport/device identity
  with authorization, lacks human MFA semantics, and makes delegation unsafe.
- **Existing `admin` role on every route:** cannot express Workspace scope,
  service identities, assurance requirements, or least privilege.
- **Implement OIDC first and copy provider groups into roles:** couples core
  authorization to one authenticator/provider and trusts ambiguous MFA claims.
- **TOTP as the privileged default:** better than password-only but phishable;
  phishing-resistant WebAuthn is the privileged baseline.
- **Recovery that disables MFA:** turns account loss into a permanent downgrade.
- **Reuse oqto-log for operator audit:** conflates Session history with security
  administration and gives the two authorities incompatible retention/access
  semantics.

## Consequences

- Remote administration is intentionally unavailable for high-risk actions until
  authentication assurance, authorization, and auditing exist together.
- Oqto must gain a real authorization boundary rather than accumulating route
  checks in handlers.
- WebAuthn is required before OIDC, or a trusted OIDC provider must be present;
  this adds deployment and UX work but avoids shipping privileged password-only
  remote administration.
- mTLS remains useful for devices, workloads, and internal runners, but never
  silently grants human administrative authority.
- `oqtoctl --host` becomes portable across local, VPS, cloud, and future cluster
  deployments because the selected target is a Control Plane, not a machine.
