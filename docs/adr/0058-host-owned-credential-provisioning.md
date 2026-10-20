# Host-owned credential provisioning and provider lifecycle

## Status

Implementation direction (2026-10-02), tracked by `oqto-3czz` and provider foundation `oqto-3czz.1`. This records the user-approved design direction, not shipped availability or completion of the dependent grant/egress work.

Refines ADR-0007 and proposed ADR-0046. Reuses `oqto-0q7x` for mediation and `oqto-aehc.1` for grants. Does not replace their authorization/enforcement requirements.

## Decision

Oqto owns one credential-management contract. Frontend and CLI provision through an authenticated, protected host path. Apps use the Host Bridge; agents use a thin native-MCP adapter; Desktop provides native presentation. These are clients of the same runner-owned authority, not independent vaults or grant issuers.

A first-class optional kyz provider and a standalone encrypted local provider share an internal SecretProvider seam. Configuration selects the provider explicitly. Locked, absent, or unsupported providers fail closed; there is no ambient environment/provider fallback.

The local provider uses the existing age encryption implementation (not a custom cipher), a human-provided vault passphrase, restrictive storage permissions, a single-writer lock and atomic durable replacement. Library/version security acceptance remains a release gate; this implementation is not an independent cryptographic audit. The foundation writes scrypt work factor 16, caps decryption at that factor, requires 16–4096 passphrase bytes, and bounds ciphertext/plaintext to 2 MiB and individual values to 64 KiB. Length is not proof of entropy: use a strong generated passphrase. Review the fixed KDF cost before production acceptance. It stores no passphrase beside ciphertext. Unlock is transient and explicit; restart requires unlocking again. Operations that perform expensive KDF/file work must run outside an async reactor. OS keychain and unattended unlock integration remain future explicit modes, not hidden plaintext files.

Provider code is a trusted internal module, not an agent-callable secret API. Its secret-bearing values have redacted Debug and no ordinary transport serialization. Resolving bytes internally does not authorize their release to a workload. Metadata also requires owner authorization at the host boundary.

For HTTP credential use, resolve and attach credentials at the final trusted destination-bound request boundary. Generated operation/MCP code and App frames receive only non-secret handles/placeholders. A real credential is never placed in their env, files, tool results or chat. Signing, OAuth refresh and cookie workflows need reviewed adapters; unsupported forms do not fall back to raw-key injection. The existing App operation host-spawn path is not an acceptable credential-bearing path.

Standalone byteowlz backends may use server-side .env or their own provider; Oqto App mode uses an explicit Oqto connection binding. No credential goes into VITE variables/browser bundles. Missing Oqto connections never activate standalone fallback. Sharing an App transfers neither credentials nor grants.

## Host-action audit integration

Metadata-only credential actions use `oqto-log`'s `host_action_audit` records through the owning Principal's runner. They are not Pi JSONL, chat messages, fabricated Sessions, or a parallel grant database. Duplicate event identities fail rather than overwrite prior records. The connection service audit adapter pins the authenticated Account and authoritative workspace partition; backend-local file writes are not an acceptable remote/placed-runner substitute. Audit failure prevents authorization. This append path does not itself authorize credential use or implement runtime/revocation fencing; those remain required at the execution gate.

## Security checklist before integration

- Derive acting Account/Principal, runtime/Instance/Definition/binding from authenticated host context; never trust App payloads, paths, AGENT_CTX or visible pane identity.
- Separate provisioning authority, credential binding, action grant, network destination grant and credential placement grant.
- Protect the submission/unlock channel: no ordinary chat/tool UI, request-body logging, analytics, URL/argv values, browser persistence or diagnostics containing inputs.
- Hide vaults, unlocked kyz/session material and another invocation's authorizations from generated workloads; same uid and HTTP_PROXY are not enforcement proof.
- Bind exact destination/protocol/port/location; prevent redirects, DNS rebinding, metadata/private targets and arbitrary authorization headers. Unsupported placement strength fails closed.
- Record secret-free lifecycle/use audit before action; do not put secret data in oqto-log or fabricate chat entries. Revocation rechecks new requests; active-stream policy is explicit.
- Bound input, decrypted data, output, deadlines and concurrency. Reject symlinks, unexpected file permissions, invalid/corrupt envelopes and concurrent writers without overwriting user data.
- Local encryption is at-rest protection, not malicious same-uid/root protection or rollback-freshness proof. Backups contain ciphertext; forgotten passphrases are unrecoverable without a separately designed recovery mechanism. Restoring a backup must invalidate/remint runtime binding authority through the future host service. Lock drops the provider's cached values/key material; it cannot retract a value already resolved into trusted mediator memory or an upstream request already delivered.
- Use synthetic credentials to prove wrong owner/Instance/runtime, locked store, tamper, rotation, revocation, restart, denied destinations and no raw-value exposure across clients.
- New canonical wire types require generated-client freshness and versioned compatibility proof. Installed Pi 1.0 native MCP is not proof that all Oqto extension/runtime lanes are qualified.

## First implementation boundary

`oqto-3czz.1` establishes only the encrypted provider foundation. It is not frontend/CLI/App/MCP integration, connection/grant persistence, live deployment, or isolation acceptance. The Linux filesystem implementation locks the directory inode (not a removable lock file), pins operations through its retained `/proc/self/fd` directory reference, and refuses changed configured-directory identity. It requires Linux proc-fd access and refuses unsupported platforms rather than advertising unproven ACL protection. The subsequent vertical slice must add authenticated runner routing and a single mediated HTTP capability before claiming useful credential integration.

`just test-credential-provider` runs package formatting, synthetic provider/lifecycle/subprocess tests, strict all-target Clippy and backend crate-boundary checks. The provider's production guardrail scan is separate from the branch-wide existing debt (`oqto-msvq`): the foundation does not modify the files with those findings, waive their rules, or establish a clean release gate. No canonical wire types changed in this slice.

## Subsequent implementation checkpoint (not credential-use acceptance)

The host now exposes personal-runner Connection create/bind/revoke/invoke routes, and the runner dispatches bounded HTTPS through `oqto-credential-http`. The authenticated host chooses Account, placement, credential descriptor, policy and audit identity; workload input contains only work-directory path, Instance, slot and relative operation. Shared/remote placement is not enabled by this implementation. New requests recheck live binding/grant/digest state after audit; already-dispatched bounded requests may complete after revocation. Runner credential resolution checks provider identity and an atomic secret/revision snapshot before attachment.

The optional session-free kyz adapter is pinned to the immutable commit behind released `v0.11.1` (`014a53c5d8478190af3e5b9dc7d31bd0f790fd8d`), not a sibling checkout or installed CLI. Default provider selection remains local; deploying this source does not migrate a vault, unlock it, change user configuration, or install kyz. Explicit kyz configuration is experimental and must not be enabled for real credentials yet: post-write revision read races, full revision representation across JSON clients, path-parent storage safety, client/provider reporting and complete route-to-TLS isolation/no-leak acceptance remain open. Unit tests and strict Clippy are not production credential-use qualification.

The App Host Bridge source now wires negotiated `oqto-app/v3` / SDK 0.4.0 `connections.invoke(slot, path)` to this existing gate. Only slot/path cross the frame boundary; the parent captures mounted Instance/work-directory identity and uses the authenticated route. The App cannot provision, bind, unlock or select credentials/providers/headers/destinations. Older mounts omit the capability; raw pre-v3 invocation fails closed. The bridge also fences late permission lookups after unmount or revocation. See `docs/development/app-connections.md` for manifest/client examples, portable SDK snapshot provenance and test evidence.

This is source wiring, not a public SDK release, deployment, real-secret use or end-to-end credential isolation acceptance. CLI and native-MCP adapters, managed SDK-store release/pin advancement, and the outstanding provider/route-to-TLS gates remain open in `oqto-3czz.2`; deploying the general platform does not close that issue.
