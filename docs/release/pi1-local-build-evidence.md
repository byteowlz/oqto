# Pi1 local Linux build — not publication/deployment approval

Tracking: `oqto-bp7p` (in progress). Integration branch: `feat/easy-deployment-pi1-build`.

## Built artifact

Local development candidate, using the checkout's declared Oqto version 0.5.0:

`/tmp/oqto-easy-release-artifacts/oqto-v0.5.0-x86_64-unknown-linux-gnu.tar.gz`

SHA256: `9e4a61b9c98bf9d4f2dc345f87a53ee2f9232eccd6473e9e38653f700d5337cd`.

Approximately 131 MiB. This is not a replacement publication of an existing tag. Any actual release must use the canonical bump/version gates and approved publication process.

Includes nine portable platform binaries, built frontend, immutable defaults, extension collection 2.2.1 at `a9a87c3d2ba0d97573941b2834fcc8497d8d83b5`, templates at `277fbdc903e21c109e1c9d89b03610e8f33fc756`, and official Pi1.0.0 archive plus extracted runtime/assets. Full-release default runner resolution follows its own immutable Pi package, including failure-on-missing-runtime rather than downgrade. Explicit user configuration remains preserve-first and needs separate upgrade acceptance.

## Evidence achieved

- Canonical manifest: 40 assets; package and full-release asset checker pass.
- Nine platform binaries: maximum referenced GLIBC version 2.28. This is symbol evidence, not a physical baseline-CPU test.
- Actual Ubuntu22.04 container: readonly artifact input, network disabled, private HOME/XDG directories and private release/bin/web roots; verified archive installation, frontend link, bundled Pi1 version, actual runner default resolving that Pi, listener startup and clean shutdown. `scripts/dist/test-packaged-ubuntu.sh` reproduces it. Strict doctor was disabled and the fixture explicitly disabled sandboxing: neither production sandbox nor full service provisioning was accepted by this test.
- Pi1 upstream RPC qualification passes: `/tmp/oqto-easy-release-pi-rpc-qualification/`.
- Shipped extensions: 14 individual startup/factory cases plus combined catalog, using exact extracted Pi1; no external service credentials. `/tmp/oqto-easy-release-staged-extensions.log`.
- Frontend selection: 26 tests, OqtoUI typecheck and zero-baseline guardrails across 166 source files.
- Earlier actual authenticated gateway/browser acceptance: `/tmp/oqto-selection-auth-browser-parent-proof/result.json`; Questions/Review, persistence, stale CAS, reload/reconnect and account/session denial. This is adapter acceptance, not a packaged full-shell browser acceptance.
- Setup unit suite: 48 cases after canonical frontend fixture correction; activation transaction suite: 15 passed, including failed upgrade restoration, strict staged doctor and reinstall safety.
- Runner unit suite: 120 passed, 1 deliberately ignored real-Pi test; separate history CLI integration suites: 2 + 2 passed. Real-Pi serialized controls have separate prior evidence.
- Final setup/runner all-target Clippy with `-D warnings` passes. Release publication gates, install-matrix safety, deploy mode/bootstrap and archive safety tests pass.

## Remaining acceptance / debts

No product-main merge, release tag, publication, live install or user-config activation occurred.

Packaged authenticated full-shell chat/Files/selection, actual profile provisioning, update with unknown user config/auth/model preservation, packaged failed activation and rollback, and baseline physical CPU qualification remain required. Installer transaction fixture tests are not substituted for those acceptance gates. Personal/network runner grants work remains independently scoped; this artifact does not claim Desktop/physical-TUI synchronized selection or all-platform acceptance.

The local portable compiler prints a Zig linker diagnostic that deprecated optimization setting `1` is ignored. Scoped source Clippy is warning-free; symbol and Ubuntu execution evidence above support retaining this local artifact, not asserting a warning-free public release. Vite's large-chunk warning likewise remains performance-budget debt, not a correctness pass. Broader extension lint and Rust guardrail debt from other lanes has not been silently waived. Public release remains blocked on acceptance/debt reconciliation.

Docker bridge networking could not create veth pairs on this host. The smoke test deliberately uses `--network none`; it does not use host networking, privileged containers or a mocked dispatcher. Full service/sandbox installation requires an explicitly authorized disposable host/VM or suitable isolated acceptance lane.
