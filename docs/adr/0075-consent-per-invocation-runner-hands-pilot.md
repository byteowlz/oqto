# ADR-0075: Consent-per-invocation Runner hands pilot

- Status: accepted for an opt-in experimental pilot, not default remote execution
- Date: 2026-10-29
- Tracking: oqto-49bm.1

## Context

The first remote-hands slice must use the existing personal Runner transport and
local-owner authority, not SSH command execution or full-user terminal grants.
Pairing establishes a peer identity; Files grants do not authorize shell execution.
A host Pi extension must not need the destination administrative socket.

## Decision

Add signed personal actions `hands_request`, `hands_run`, `hands_status` and
`hands_cancel`. They use the existing nonce/signature checks and pinned HTTPS.
Requests are available only when a Linux destination explicitly enables the provider
with `--personal-hands-bpf`. No Oqto backend is required.

Every command requires exact destination-owner consent through the existing private
owner socket. The owner chooses the execution root, independently of any Files grant.
The initial approval lasts 30 seconds and is for one invocation only. A request waits
at most five minutes. The source Session label is explicitly unverified attribution;
the authenticated identity is the paired device. This pilot does not claim a
Runner-attested mapping of that label to a platform Session.

The authority owns a separate migration-backed invocation table. A `(device_id,
request_key)` identifies immutable command input: duplicates resolve to the same ID;
changed input is rejected. A transactional transition to `running` precedes spawn.
A transport retry never makes it runnable again. There are at most four running
claims and 1024 stored invocations per device in this pilot; no automatic retention
or queued scheduling is introduced.

Execution belongs to Runner. The provider always uses strict bwrap isolation with
an explicitly compiled, enforced seccomp filter, private `/tmp`, no network, empty
ambient environment, workspace-only write additions and no terminal. It does not
read project policy to widen these permissions. Authority roots, the Runner binary,
filter and configured shim cannot be included in its writable root. It refuses
unsupported platforms or unavailable confinement; no local/full-user fallback.

Execution is detached from the HTTP request's lifetime so a dropped response does
not orphan the work or trigger a retry. It is capped at 30 seconds and expiry is
also checked during execution. Cancellation and device revocation fence delivery and
stop the sandbox. Device admission guards are released before execution so owner
revocation does not wait behind a long-running command. Kernel resource isolation is
not equivalent to a microVM or comprehensive host DoS protection.

Output is drained with bounded retention (30 KB per stream, explicit cap marker),
UTF-8-safe delivery and a durable receipt. Revoked/expired output is withheld.
Cancellation may have prior effects and is labelled uncertain; restart marks any
previous `running` claim uncertain instead of replaying it. These are conservative
no-replay semantics, not an exactly-once guarantee for arbitrary external effects.

Ordinary Pi can explicitly load `tools/oqt/pi-hands/index.ts`. It registers
`hands_bash` for one owner-configured environment and captures native Session plus
tool-call identity into the request key. It polls consent and receipts; abort/shutdown
attempt destination cancellation. It does not replace stock Pi tools, isolate the
host harness or expose an unrestricted Runner capability. The private peer credential
stays with host Pi for this pilot: this is not yet oqt's confined-brain broker adapter.

## Consequences and boundaries

- A real Session/tool -> signed HTTPS -> destination approval -> confined execution
  path is testable without touching user profiles or installing services.
- The same command ID is inspectable on the destination, with no credentials in
  request arguments. Owner listings JSON-escape command text and unverified labels.
- No popup/UI, reusable environment grant, remote file transfer, ephemeral provisioning,
  scheduler, automatic Session sync or generic remote ExecutionEnv is claimed.
- No live Mac service was changed. This provider is Linux-only. The native Mac
  client was separately built and exercised against Linux over the tailnet; that
  does not qualify native macOS execution or a Mac-confined oqt brain.
- The existing native Pi/oqto-log history authorities are unchanged. The pilot only
  adds tool results to the source Session through its normal harness API.
- New actions are additive to the strict personal action enum. Unsupported old peers
  reject them; existing capability responses are unchanged. No frontend generated
  HostAction bindings exist to regenerate for these experimental operations.

## Verification

See [pilot runbook](../development/runner-hands-pilot.md). Tests use disposable keys,
state, workdirs, pinned loopback HTTPS and real Linux confinement. A synthetic-model
Durable Session calls the actual Pi extension tool and obtains a real remote Runner
result. An additional isolated Mac-client/Linux-destination test proves physical
networking, consent, no duplicate effect and peer revocation. Neither test is a
persistent deployment, Mac-confined oqt brain proof or qualification of every native
Pi UI mode.
