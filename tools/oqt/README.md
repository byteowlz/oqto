# oqt: source-only permission TUI demo

```sh
cd /home/wismut/byteowlz/oqto-durable-support
./oqt
# Reopen the latest demo Session for this work directory:
./oqt --continue
```

`oqt` opens **upstream Pi's experimental Durable TUI**, not another renderer. This milestone is explicitly a **demo, not a usable coding/model profile**:

- `own history`: baseline access to a synthetic history fixture.
- `global history`: scoped request for the synthetic account-history fixture. The TUI presents **Allow once / Allow for this Session / Deny** while the original turn is waiting. Deny is the default.
- `read file`: request for a registered fixture file; approval invokes actual bwrap/seccomp-confined hands.
- `/permissions`: revisit a pending request after dismissing its dialog.
- `/permissions revoke`: revoke approved overlays. Subsequent access asks again. Already disclosed data cannot be erased; baseline policy remains available through fresh admission. Every in-flight permit is revision-fenced.

Session approvals expire after **60 seconds** in this demo. Ordinary read/edit/bash/codemode tools, arbitrary plugin reload, model credentials, real history/mmry/trx providers, and Oqto authentication are **not enabled**. The synthetic model ignores arbitrary query/path text and only selects those registered fixtures. Never interpret this as a secure general-purpose coding session or an authenticated Oqto Session.

## Composition and isolation

The launcher requires Linux x86_64, bwrap, cached Cargo dependencies, Node 26, and the qualified source checkout at `cdaf6c227d4718a72b59ccd02152c9614df44cfc`. Override its location with `OQT_PI_SOURCE`; wrong/dirty pins fail closed.

A private source distribution is created under `/var/tmp/q<uid>-<artifact>/`. It reuses upstream server/protocol/worker/harness/client/TUI code. Its application entry composes the public `runSessionWorkerWithHarness` factory seam with our gated registry instead of `createCodingAgentHarness`. Automatic server activation in that private distribution therefore cannot select stock local tools. Original upstream source and installed Pi remain unchanged.

Only the distribution's controlled TUI package and static provider facet are configured. The provider owns separate private tool/operator carrier pipes. Tool frames cannot inspect/decide/revoke. Decisions bind the reviewed request digest and revision; authority rechecks before fixture access and result delivery. Public demo Session ID is distinct from native conversation/tool-task IDs; request correlation binds both plus the protected ledger namespace. Native Durable SQLite remains transcript/task authority; the permission ledger is not another Chat history.

Local operator trust is this prototype's private OS-owned Unix/profile boundary. Account labels are synthetic, with a local UID Principal marker, **not production Account authentication**. The brain is trusted and unconfined; only the fixture hands are confined. This is not the authenticated runner integration gate.

Runtime/profile directories are private and hash-keyed; unknown or non-private directories, workspace collisions and runtime edits are refused. Unix socket paths are bounded below Linux's 108-byte limit. Runtime artifact changes produce a new profile; demo Sessions are not automatically migrated across artifacts. Files under existing `~/.pi`/`~/.config`, ordinary Pi JSONL and installed commands are not rewritten. No PATH installation, service activation, release tag, or deployment occurs.

Warm launches cache Cargo preparation and seccomp generation by source/toolchain inputs and artifact identities; carrier hashing is cached by file identity. These are performance caches, not authority or signatures. Runtime source hashes are still checked. Build/preparation phases use one CPU, nice 19, one Cargo worker and 90-second deadlines. The interactive session inherits affinity/nice and a 512 MiB V8 heap ceiling (not an aggregate memory quota). Closing the TUI closes its source-only server and workers. Simultaneous-client launcher/reuse and abnormal termination/recovery are not qualified here.

## Real-session groundwork (not activated)

`oqt_hands.rs` starts the actual Pi `RemoteExecutionEnv` daemon inside enforcing bwrap/seccomp, with a trusted private spec, workspace read/write, no network, clean environment, explicit protected roots and no host-local fallback. An independent native transport test proves workspace FS/shell work and outside/authority/symlink reads fail. The transport is connected to the normal coding registry and permission ledger by `coding-runtime.ts`, qualified with a synthetic provider in native Durable turns. Its strict policy does **not** qualify the production global/workspace ceiling compiler or protected brain. Real-model launcher activation remains disabled.

`readonly-profile.ts` provides an owning-machine read-only credential interface: no profile writes, credential commands, logout, or OAuth refresh side effects. It opens owned private regular files without following the final symlink, bounds reads on the opened descriptor, and rereads each invocation. Static API keys and existing OAuth access tokens are supported; refresh secrets are removed from the returned credential. The exact upstream OAuth resolver is tested: valid tokens resolve with account metadata; expired/near-expiry tokens fail before its refresh callback; no ambient API-key fallback occurs after that failure. An atomic credential replacement by the owning Pi becomes visible on the next invocation. This is a **read-only bridge**, not an automatic refresh broker: renew through owning Pi when needed. No original credential is copied to a new profile. The interface is not connected to the launcher yet, and no live provider request is claimed.

`owner-models.ts` reads owning Pi model defaults and exposes an upstream `ModelsStore` with owner catalog reads plus an in-memory write/delete overlay. Runtime cache updates never create locks or modify the original catalog; a new runtime starts again from the owner's catalog. An exact upstream `ModelRuntime.create` fixture test composes a custom provider/model and derives its auth with no profile writes. This is local composition evidence, **not** a live provider turn. Normal coding registry/launcher integration remains disabled.

`confined-coding.ts` composes only upstream `createCodingTools()` with `RemoteExecutionEnv`: read/write/edit/bash, no local environment factory, no codemode/skills/plugins/subagents. Conversation `cwd` must match the owner-bound workspace; missing/different targets fail. Native Durable proof runs a full write → edit → read → bash turn, verifies actual filesystem effects, and checks an outside authority read becomes a tool error with no secret in durable/model history. The original ungated qualification remains isolation-only; optional gate composition wraps every tool. `coding-runtime.ts` is the fully gated local composition, not the launcher default. Do not activate it for real models yet.

`coding-authority.ts` uses separate tool/operator carrier pipes. The Rust carrier holds operation-bound, non-deserializable permits in a bounded live-invocation map; wire commands cannot forge them. Every call waits for exact argument/digest/revision approval, rechecks immediately before execution and result handoff, and releases its live permit. Coding policy supports only `ask` and `deny`; unsupported values fail closed. This is a fixed conservative ceiling, not a production policy compiler.

`gated-tool.ts` withholds synchronous output, details and diagnostics in a bounded buffer until delivery authorization. A revoked, aborted or failed operation emits a sanitized error, not executor data. All coding tools use native `replay: "unsafe"`: interrupted effects are uncertain and never blindly replayed. Output exceeding 1 MiB fails rather than committing unchecked progress. This trades live streaming for an enforceable delivery boundary.

`hands-supervisor.ts` owns the actual launcher child handles behind a private Unix transport relay. Revocation freezes authority, advances the ledger revision, stops hands, waits for confirmed exit, and installs fresh environment/connection handles. Retained handles cannot reconnect. The shell cannot access the protected socket/spec; its stdout/stderr cannot act as supervision messages. Failure to confirm stop leaves admission and delivery frozen. The launcher retains a fixed PATH including `/usr/local/bin` for the enforcing Landlock shim; missing enforcement helpers fail closed, never fall back.

The composed native proof executes a real shell that outputs a secret and starts a detached writing descendant, revokes its grant while running, verifies no secret reaches durable/model history, verifies the descendant stops, and then approves a write through fresh hands **within the same native turn**. Standalone tests also cover all four coding tools, hard denial before effects, tool-channel control rejection and stale environment handles. The foundation gate now has **11 passing tests**. This is not a live-provider, authenticated Oqto, brain-confinement, external-resource expansion, reopen/kill-recovery, or UI-launcher acceptance claim.

## Proof

```sh
./oqt --smoke
just test-oqt-ui
# Independent transport/credential foundation, with explicit exact-source artifacts:
just test-oqt-hands /tmp/oqto-pi-service-spike.PVTso2/source \
  /tmp/oqto-pi-env-qualified-target/debug/pi-env \
  backend/target/oqt-prepared/bpf/default-x86_64.bpf
CARGO_TARGET_DIR=/home/wismut/byteowlz/oqto_refactor/backend/target \
  just test-adaptive-sandbox /tmp/oqto-pi-service-spike.PVTso2/source
```

The PTY gate drives the actual TUI: history Once approval inside the waiting turn, file denial, and file Session approval followed by live revoke and a new denied request. It verifies the safest default and completed native turns. This is PTY coverage, **not physical terminal/Web/multi-client acceptance**. Logs contain only demo fixture data and are written under `/tmp/oqt-*-pty.log`.

Core/native gate: 23 lifecycle tests + actual confined replacement + 3 native Durable tests (27 total). Rust Clippy and owned TypeScript Biome are warning-free.

Qualification debt: a full source TypeScript check initially omitted upstream ambient highlighter declarations; the corrected source check still encounters three existing OpenAI response SDK type mismatches (`prompt_cache_options`, `fast`). No owned demo-file type diagnostics were reported. Full upstream typecheck is **not claimed**. This prototype uses only the synthetic provider, so accepting this isolated source-only demo does not qualify real-provider or production use; that gate remains blocked until the pinned upstream dependency/type surface is qualified. Real coding must add verified runner authority, stable brain confinement and confined ExecutionEnv coverage for every local execution path before enabling normal tools.
