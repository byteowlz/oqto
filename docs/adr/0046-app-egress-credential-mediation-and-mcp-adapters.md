# App egress, credential mediation, capability endpoints, and MCP adapters

## Status

Proposed (2026-09-04). Tracked by `oqto-ft2m`.

Refines [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md), [ADR-0039](0039-capability-endpoints-are-placement-neutral.md), [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md), and proposed [ADR-0045](0045-unified-app-actions-and-host-owned-action-surfaces.md). Reuses `oqto-0q7x` for the SecretProvider/credential-channel implementation. It does not make MCP, a proxy, or an App the authorization authority.

## Context

Agent-authored Apps must integrate arbitrary APIs without Oqto implementing one provider-specific connector for every service. An ElevenLabs App should be able to use a normal SDK or an existing MCP server, while the App presentation, generated operation code, and Workspace never receive the real Account credential.

Oqto already separates egress authorization, credential mediation, and traceability in ADR-0035. Its iron-proxy spike proved destination-bound placeholder substitution, structured audit, HTTP/HTTPS/WebSocket/SSE/HTTP2 support, and a `--network=none` placement whose only route out is a granted endpoint socket. ADR-0039 defines the placement-neutral rule: capability providers run where authority exists and workloads receive scoped endpoints, never secret material.

The missing proposal is how Apps consume this machinery, how opaque browser presentations differ from operation/MCP processes, and how MCP tools map into the shared App Action model without creating a second grant authority.

## Proposal

### Preserve three independent decisions

Every request evaluates three orthogonal concerns:

1. **Egress authorization:** may this Principal/Instance reach this protocol, destination, and delegated capability?
2. **Credential mediation:** may this request receive one exact bound secret at one declared location?
3. **Action authorization:** may this Account/Work Session/App Instance invoke this semantic Action against these resources and revisions?

They may share durable grant facts and audit correlation IDs, but none substitutes for another. A successful Action grant does not open arbitrary network; an allowed destination does not grant a secret; possession of a credential placeholder is not authorization.

### Generic egress is primary; provider-specific connectors are optional

Oqto does not require a built-in ElevenLabs connector. An App may request bounded HTTP-shaped egress to named destinations and named credential slots. The Account binds a slot to a SecretProvider entry separately from granting the App.

Conceptually:

```toml
[capability.egress]
mode = "allowlist"

[[capability.egress.destination]]
id = "elevenlabs-api"
protocol = "https"
host = "api.elevenlabs.io"
ports = [443]

[[capability.egress.credential]]
slot = "elevenlabs-account"
destination = "elevenlabs-api"
placement = { kind = "header", name = "xi-api-key" }
```

This is a request, not a grant. Publication validates syntax and combination risk; the authorized Account sees the destination, data classes where declared, credential binding, and files+egress combination before deciding. The App sees only whether the slot is configured and an opaque random placeholder scoped to the grant. It never receives a vault path or secret value.

Provider profiles remain useful as optional reviewed presets for safer defaults, OAuth flows, richer cost controls, or signing semantics. They are not required for ordinary bearer/API-key integrations and cannot become an expressiveness ceiling.

### Placeholder substitution occurs only at the trusted credential channel

For operation, Sidecar, or MCP workloads, Oqto injects a random non-secret placeholder through bounded environment/config and exposes the credential endpoint. The client uses an ordinary HTTP library. At the final trusted egress hop, the credential channel:

- resolves the live grant and SecretProvider binding;
- corroborates exact protocol/destination/port and resolved-address policy;
- replaces only the exact scoped placeholder at its declared header/query/body location;
- disables or independently reauthorizes redirects;
- denies loopback, link-local, metadata, and configured CIDRs;
- strips/redacts the secret from request diagnostics, responses, errors, and audit payloads;
- applies request/response byte, duration, rate, concurrency, and optional cost limits;
- records Account, Principal, Workspace/work directory, App Instance/Definition, Action/invocation, destination, secret slot, placement, verdict, and byte counts without recording the secret;
- rechecks revocation for new requests and terminates streams according to lifecycle policy.

Skipping the credential channel may not reveal the secret. Under enforced placements it also provides no alternate egress route. Under an honestly advertised `none` tier, placeholder secrecy still holds but destination authorization against a non-cooperating process is not claimed.

Exact secret-byte response filtering is defense in depth, not the primary guarantee. Redirect denial, destination binding, controlled placement, provider behavior, and least privilege remain load-bearing. Capability-combination review must state that files plus egress can transmit granted file content to the approved service.

### Capability endpoints and sockets

On Linux/local placements, the credential channel arrives as a Runner/Placement-Supervisor-owned Unix-domain capability endpoint, commonly projected under `/run/oqto/endpoints/`. It is never the Runner control socket, never stored under a work directory, and never addressable by an App-chosen host path.

Endpoint scope is the finest boundary the Placement proves it can enforce: per invocation/session where nested isolation exists, otherwise per Workspace. A finer App grant cannot be claimed on a coarser shared socket. Endpoint creation, mount/bridge, permissions, environment naming, health, revocation, and teardown are Supervisor/Gate responsibilities.

The endpoint may be bridged to loopback inside a `--network=none` placement for conventional proxy-aware clients. The Unix socket remains the placement capability; loopback reachability is not authorization. Remote placements multiplex capability traffic over the authenticated runner channel rather than opening another listener.

HTTP-shaped traffic uses the bought iron-proxy credential/enforcement path from ADR-0035. SSH uses the SSH-agent/signing proxy plus explicit reachability. PostgreSQL or other supported protocols use their reviewed mediator. Arbitrary non-HTTP TCP requires an explicit destination endpoint or is denied.

### Opaque presentations use a transport-neutral Host HTTP capability

A sandboxed-web presentation cannot open Unix sockets and must not receive a directly usable network bearer token or unrestricted Oqto-origin fetch route. It uses a fetch-like, bounded Host capability over its private MessagePort:

```text
host.http.request(destination_id, request) -> bounded response or resource ref
host.http.stream(destination_id, request)  -> flow-controlled chunks
```

The Host adapter attaches authenticated Instance/Definition/Account/Presentation Context facts; the App cannot supply them. The backend and Runner resolve the same egress/credential grant and endpoint used by non-browser workloads. The interface accepts relative paths beneath the granted destination, method, bounded headers/body, and declared response mode. It rejects absolute alternate origins, credential headers, ambient cookies, redirects, and unsupported streaming.

Large request/response bodies use bound resource references and streaming rather than crossing the Bridge as one JSON payload. Generated audio, video, images, and documents should normally be written to a granted output resource and returned as typed references.

### Operations and Sidecars use ordinary clients

A pinned App operation may use curl, an official SDK, or another HTTP client through the scoped credential endpoint. App-authored executable code receives only placeholders. An App Sidecar follows the same rule and is available only at trust/placement tiers that explicitly support it.

Operation execution must actually occur inside the placement/sandbox to which the egress policy and endpoints apply. A generic Runner spawn path that executes on an unrestricted host network is not compliant merely because its caller was grant-checked.

### MCP is an Action implementation adapter

MCP is encouraged when an existing server provides useful semantic tools, pagination, streaming, or provider-specific behavior. Oqto may supervise local MCP servers over stdio or scoped Unix sockets, or reach remote MCP over its authenticated HTTP transport through the same egress policy.

An App Action may bind to an MCP tool conceptually as:

```toml
[[action]]
id = "elevenlabs.speech.generate"
input_schema_file = "actions/speech.generate.input.json"
output_schema_file = "actions/speech.generate.output.json"
handler = { kind = "mcp-tool", provider = "elevenlabs", tool = "text_to_speech" }
```

Oqto snapshots and validates the exact tool/schema mapping into the Definition or reviewed provider binding. Dynamic server changes cannot silently alter a pinned Action. MCP descriptions/results are untrusted data, not instructions.

MCP handles tool interoperability; Oqto retains identity, Installation/Instance/Definition binding, grants, resource handles, expected revisions, lifecycle, disclosure, audit, context reduction, command/menu/share projections, and target Principal enforcement. MCP servers do not receive the Runner control socket or choose acting identity.

A tool server that expects an API-key header may receive a placeholder and use the credential channel. A trusted remote OAuth MCP provider may use its protocol-native authorization through an Account binding. A local untrusted MCP server never receives raw long-lived credentials merely because it uses MCP.

### Secrets requiring local cryptography use signing endpoints

Placeholder replacement covers bearer tokens, API-key headers, query values, and similar byte placement. It does not cover protocols where the client must possess a secret to calculate HMACs, AWS SigV4, client certificates, or private-key signatures.

Those use a purpose-built signing endpoint analogous to the SSH-agent proxy. The workload submits bounded sign/exchange requests; the provider applies operation, identity, destination, and key policy and returns only the signature or short-lived credential. Unsupported signing schemes fail explicitly rather than materializing a key as fallback.

### Actions remain host-neutral

The same ElevenLabs capability may be invoked from its App, an Agent, a context menu, a command palette, a Transformation Run, an iOS Oqto share flow, or headless CLI. All surfaces resolve one App Action and invocation identity. The wire behind the handler may be direct HTTP through the credential channel, an MCP tool, or a reviewed native adapter; callers do not need to know.

Example trace:

```text
Share audio → Oqto stages scoped resource
→ Action Broker matches installed elevenlabs.audio.transcribe
→ Account selects Action
→ Gate validates App/Action/resource/egress/credential grants
→ Runner invokes pinned MCP tool or operation in target Placement
→ client sends placeholder-authenticated request through capability endpoint
→ iron-proxy injects Account secret only for api.elevenlabs.io
→ transcript is written to bound output resource
→ Broker returns resource reference and audit correlation
```

### Lifecycle and failure behavior

Revocation, suspension, uninstall, Definition change, credential unbinding, Account/Workspace authorization loss, Work Session end, or Placement teardown invalidates placeholders and endpoints. Reconnect remints scoped placeholders after durable authorization; browser, MCP, or process state cannot resurrect them.

Missing SecretProvider, unavailable credential endpoint, unsupported protocol, unmet enforcement tier, absent MCP provider, schema drift, or failed placement probe degrades to an unavailable capability or refuses the Action according to policy. It never widens egress, injects a different credential, starts on host networking, or returns the raw secret.

## Consequences

- Agents can build integrations with ordinary SDKs without Oqto shipping a provider connector for every API.
- Oqto reuses iron-proxy and capability endpoints rather than writing a new HTTP parser/proxy.
- The SDK needs a bounded Host HTTP/stream interface; operations need placement-enforced egress attachment.
- A SecretProvider trait supports kyz, Vault, Infisical, environment/file sources, or operator adapters without exposing provider paths to Apps.
- MCP broadens interoperability but adds supervision, version/schema pinning, health, and lifecycle work.
- Files+egress and resource+MCP combinations require explicit human-readable review and audits.

## Rejected alternatives

- **Provider-specific connectors as the only path:** safe but an artificial ecosystem ceiling and duplicated SDK work.
- **Raw secrets in App frames, operation environments, Sidecars, or MCP servers:** generated code can read and exfiltrate them.
- **Literal substitution in arbitrary App-selected destinations:** turns the credential channel into an exfiltration oracle.
- **MCP as the grant or context authority:** lacks Oqto Account/Principal, binding, revision, lifecycle, and host-surface semantics.
- **Direct App iframe access to endpoint sockets or extra ports:** impossible across hosts and bypasses the private Host Bridge.
- **A work-directory control socket:** writable/mountable source is not authority and may expose Runner control.
- **Assume `HTTP_PROXY` enforces egress:** non-cooperating runtimes can bypass it; enforced tiers remove the alternate network path.
- **Put every protocol through an HTTP CONNECT tunnel:** unsupported protocols need their own mediator or explicit endpoint.
- **Fallback to a real key when mediation fails:** converts availability failure into credential compromise.

## Implementation slices

1. Complete `oqto-0q7x`: SecretProvider binding model and iron-proxy policy compiler for random scoped placeholders.
2. Capability-endpoint lifecycle and placement conformance for credential channels, including truthful scope advertisement.
3. Route App operation execution through the actual placement egress policy; close unrestricted spawn bypasses.
4. Add manifest request, grant UI, audit correlation, and runtime rechecks for App egress/credential capability combinations.
5. Add bounded SDK/Host HTTP request and resource-backed streaming adapters for opaque presentations.
6. Add MCP provider supervision/adapter and pin one tool mapping into an App Action.
7. Build an ElevenLabs fixture using ordinary HTTP, then the same Action through MCP, proving equivalent authority and outputs.
8. Add signing-endpoint interface only when a concrete non-placeholder integration requires it.

## Verification

1. A fixture workload receives only a random placeholder; process environment, filesystem, stdout/stderr, Bridge frames, browser state, and audit records contain no real secret.
2. The proxy substitutes only for the exact granted destination/location and rejects alternate host, port, protocol, redirect, DNS rebinding, private/link-local/metadata address, malformed HTTP, and revoked placeholder.
3. Direct socket/raw-IP/DoH/proxy-ignoring attempts fail from an enforced placement; the same node advertises `none` if the denial probe cannot pass.
4. Files+egress review names the combined data-flow risk and revocation terminates future requests without deleting bound outputs.
5. Local, bwrap, container, remote, and later microVM adapters pass the capability-endpoint matrix or explicitly advertise unsupported/coarser scope.
6. Opaque App presentation, pinned operation, and MCP handler invoke one ElevenLabs Action through different adapters and receive equivalent typed resource outputs without credentials.
7. MCP schema/tool drift fails before invocation; a server cannot select Account, Principal, resource grant, Action identity, or destination.
8. Large audio streaming is flow-controlled and bounded; disconnect/cancel stops upstream work according to declared semantics and leaves no orphan authority.
9. Placeholder-incompatible signing fails explicitly until a signing endpoint exists; tests prove no raw-key fallback.
10. Lifecycle tests cover grant revoke, credential unbind, Instance suspend, Definition change, uninstall, Work Session end, endpoint teardown, reconnect, and remote partition.
11. Audit joins Action and egress events through one correlation identity while redacting request secrets and sensitive payloads.
12. Delegated-fetch fields and provider-side tools are separately granted and denied by default where the destination can fetch on behalf of the workload.

## Open questions before acceptance

- Exact manifest vocabulary for destinations, credential placement, and declared data classes.
- Whether Host HTTP v0 supports streaming directly or only resource-backed request/response bodies.
- MCP provider Installation/version pinning and who may approve untrusted local servers.
- Rate/cost policy ownership for paid APIs.
- Which placeholder locations iron-proxy supports without custom request transforms.
