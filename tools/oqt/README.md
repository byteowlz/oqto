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

Build/preparation phases use one CPU, nice 19, one Cargo worker and 90-second deadlines. The interactive session inherits affinity/nice and a 512 MiB V8 heap ceiling (not an aggregate memory quota). Closing the TUI closes its source-only server and workers. Simultaneous-client launcher/reuse and abnormal termination/recovery are not qualified here.

## Proof

```sh
./oqt --smoke
just test-oqt-ui
CARGO_TARGET_DIR=/home/wismut/byteowlz/oqto_refactor/backend/target \
  just test-adaptive-sandbox /tmp/oqto-pi-service-spike.PVTso2/source
```

The PTY gate drives the actual TUI: history Once approval inside the waiting turn, file denial, and file Session approval followed by live revoke and a new denied request. It verifies the safest default and completed native turns. This is PTY coverage, **not physical terminal/Web/multi-client acceptance**. Logs contain only demo fixture data and are written under `/tmp/oqt-*-pty.log`.

Core/native gate: 23 lifecycle tests + actual confined replacement + 3 native Durable tests (27 total). Rust Clippy and owned TypeScript Biome are warning-free.

Qualification debt: a full source TypeScript check initially omitted upstream ambient highlighter declarations; the corrected source check still encounters three existing OpenAI response SDK type mismatches (`prompt_cache_options`, `fast`). No owned demo-file type diagnostics were reported. Full upstream typecheck is **not claimed**. This prototype uses only the synthetic provider, so accepting this isolated source-only demo does not qualify real-provider or production use; that gate remains blocked until the pinned upstream dependency/type surface is qualified. Real coding must add verified runner authority, stable brain confinement and confined ExecutionEnv coverage for every local execution path before enabling normal tools.
