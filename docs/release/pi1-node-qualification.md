# Exact Pi1 Node runtime qualification

Tracking: `oqto-bp7p`, CPU evidence on `oqto-ffv5`. Isolated branch `feat/easy-deployment-pi1-build`; no live install/config/service change, tag or publication.

## Decision supported by evidence

The illegal-instruction failure reproduced in the official Pi1 standalone Bun executable is **not a Pi1-wide requirement**. Official Pi1 supports its npm CLI (`dist/bundle/cli.js`) on Node>=22.19.0. Exact Pi1.0.0 on Node24.14.1 passes substantially more than version startup under the same conservative `qemu64` profile. Do not impose an Oqto-specific x86-64-v2 floor based on the Bun executable's failure. This is not a claim that every x86-64-v1 machine or every Node version is supported.

## Provenance and reproducible preparation

- Pi: official `@earendil-works/pi-coding-agent@1.0.0` from npm, not a custom Pi build or older version. Integrity: `sha512-/FtbxoSQU/mEv1QnichJjRjqteqaIaMWxmhB4G367+MwZfX7/DI5B9YAg5lqbN7nztFskBEtUSZ+FlmMBECtMw==`.
- Node: official `node-v24.14.1-linux-x64.tar.xz`, matched publisher SHASUMS256.txt. SHA256 `84d38715d449447117d05c3e71acd78daa49d5b1bfa8aacf610303920c3322be`. Maximum referenced GLIBC symbol in node executable: 2.28. Publisher signature qualification remains a separate release gate.
- Exact closure committed in `scripts/dist/pi-node/package-lock.json`: 147 package entries, HTTPS npm-registry sources and SHA512 integrity for every fetched dependency. Upstream's shrinkwrap-derived lock omitted integrity for seven nested Earendil1.0.0 packages; resolved exact-version publisher metadata supplied their missing checksums without changing versions. Unit test prevents recurrence.
- `scripts/dist/stage-pi-node-runtime.sh` checks pins/hash, installs via the pinned Node/npm with `npm ci --ignore-scripts`, isolated HOME/cache/empty config and stripped environment. Unknown/existing output refuses replacement. Failed hash leaves no promoted runtime or scratch stage. Three tests pass (`just test-pi-node-runtime`).
- Complete runtime/assets/Node licenses retained. Installed size approximately627MiB; optimization/pruning is not yet qualified. Installer emits upstream transitive `node-domexception@1.0.0` deprecation: retained explicit upstream dependency debt, not hidden or called warning-free.
- Launcher follows managed symlinks and invokes its own `node/bin/node`; fake incompatible `node` in PATH did not affect launch. Damaged runtime must not fall back to global Node/Pi.

Prepared immutable-candidate tree:
`~/byteowlz/oqto-release-artifacts/pi1-node-qualification/runtime`.
Receipt: `oqto-runtime.json`. The original Bun 0.5.6 tarball is unchanged. The default release staging/workflow and standalone archive acquisition helper have **not yet switched** to Node. They need source-integrated identity/checksum semantics and full packaged transaction proofs; do not label this tree a published or accepted full release.

## CPU/emulated runtime evidence

Reproducer:

```sh
scripts/dist/test-node-pi-cpu.sh \
  /tmp/oqto-cpu-proof/ubuntu-qemu/usr/bin/qemu-x86_64-static \
  ~/byteowlz/oqto-release-artifacts/pi1-node-qualification/runtime \
  /tmp/oqto-easy-release-extensions \
  dist/immutable/defaults/pi-agent/extensions
```

Pinned Ubuntu22.04 container, UID65534, no capabilities, no-new-privileges, read-only root and mounted runtime/extensions, network disabled, private writable HOME/tmp, bounded termination. QEMU6.2 runs the **main Pi Node process** as qemu64. Harness/mock provider and external tool executables run natively in the same isolated container; QEMU user-mode does not automatically emulate every spawned subprocess. No physical CPU acceptance claim.

Passed on exact locked staged tree (`/tmp/oqto-pi1-node-qualification/locked-qemu64-proof.log`):

- Pi version1.0.0 startup.
- All14 individually shipped extensions and combined catalog: imports/factory/session-start, discovered tools and actual RPC-mode command handler. No extension-load error.
- Selection controls with and without capabilities: durable answers, real store, structured results and zero model/chat/history messages.
- Actual Selection tool callback via local synthetic streaming provider, follow-up answer, durable record and clean agent settlement. No external provider or credentials.

These are not proofs of every extension tool callback, TUI keyboard interaction, browser/model deployment or SQLite history-index responsiveness.

## Native Node RPC and actual runner evidence

`test-pi-rpc-qualification.py` on the locked staged launcher passes (`/tmp/oqto-pi1-node-qualification/locked-native-rpc/report.json`): correlated responses, delta/LF Unicode framing, prompt disposition, queued follow-up settlement, manual compaction, abort settlement, system transcript, native session identity and RPC/disk entry-ID agreement. Real local synthetic provider; Pi alone owns its JSONL. Not an oqto-log/translator acceptance substitute.

`test-node-pi-runner.sh` passes actual0.5.6 portable runner startup on unprivileged, network-disabled Ubuntu22.04. It resolves Node Pi through the canonical executable-relative `immutable/pi-runtime/pi/pi`, starts its private Unix listener and shuts down cleanly. Source runtime mount is read-only; sandbox deliberately disabled. First fixture attempt used root-owned tmpfs as socket parent and correctly failed chmod; final fixture uses a UID-owned private socket directory. This proves native resolver/startup, **not complete package activation, authenticated agent creation or sandbox enforcement**.

Evidence log: `/tmp/oqto-pi1-node-qualification/runner-startup.log`.

## Bun audit and remaining work

Shipped Pi extensions' runtime audit found history indexer supports `node:sqlite` as well as `bun:sqlite`. Pi loader successfully loads every shipped extension under Node. The separately advertised **standalone selection CLI** still has a Bun shebang and source `.js` imports resolved by Bun: bare `node cli.ts` fails module resolution. Do not claim that command is Node-ready. Oqto selection RPC/control does not execute that CLI and passed without Bun available.

Remaining: integrate Node into canonical full-release staging/acquisition with matching checksum contract; clean packaged install/update/failed-activation rollback/config preservation; authenticated full-shell chat/Files/selection; physical CPU and worker/native helper coverage; actual provider readiness and profile/service/sandbox acceptance; signing/provenance gates. No automatic downgrade, dynamic latest lookup or hosted history inference.
