# Pi1 0.5.6 acceptance checkpoint

Tracking: `oqto-bp7p` remains in progress. No publication, tag or live activation.

## Candidate

The public release inventory (GitHub API, 2026-10-03) includes versions through v0.5.5. The earlier 0.5.0 artifact was a development baseline, not suitable for re-publication. `just bump 0.5.6` advances workspace/frontend/Tauri versions; the canonical bump now also synchronizes `[oqto].version` in `dependencies.toml`. Three preservation/refusal/idempotence tests cover that manifest update. Pi stays exactly official **1.0.0**; extensions/templates pins are unchanged.

Artifact: `/tmp/oqto-pi1-056-artifacts/oqto-v0.5.6-x86_64-unknown-linux-gnu.tar.gz`.
Durable copy: `~/byteowlz/oqto-release-artifacts/pi1-dev-056-68b4801b/oqto-v0.5.6-x86_64-unknown-linux-gnu.tar.gz`.
SHA256: `68b4801b09e5812fe901356460b2159ac138ebe0ab7e10171d891f8db5ae5427`.

Binaries and frontend rebuilt, package/asset/version gates pass. Setup/runner all-target Clippy with `-D warnings` and full scoped tests pass at 0.5.6. Portable linker and frontend chunk diagnostics remain as documented in the earlier local-build evidence; not waived for public release.

## Actual packaged transactions

`scripts/dist/test-packaged-upgrade.sh` uses two canonical development artifacts: the earlier locally built 0.5.0 baseline and 0.5.6. It runs as UID65534 with no capabilities, read-only container root, network disabled and disposable executable tmpfs install paths on Ubuntu24.04.

Passed (`/tmp/oqto-056-canonical-upgrade-proof.log`):

- Original 0.5.0 native installer activates its own complete package.
- Real staged 0.5.6 `oqtoctl` rejects an explicitly selected invalid doctor config **after activation**. Native installer restores original current/last-good and managed executable link; 0.5.0 runs again. No fake doctor binary or alternate activation implementation.
- Subsequent real 0.5.6 upgrade succeeds with doctor delegated, serving packaged frontend and matching bundled Pi1.
- Unknown user config, Pi auth and settings remain byte-identical through failed activation and successful upgrade.
- Identical reinstall is **refused**, as current installer policy requires. Active release/frontend and config remain intact. This is safe refusal, not successful no-op idempotence.

The 0.5.6 Ubuntu22.04 packaged activation/default bundled-runtime startup/shutdown smoke also passes (`/tmp/oqto-056-ubuntu22-proof.log`). Neither fixture proves service provisioning, live model/chat, sandbox enforcement or full-shell browser acceptance.

## Published predecessor blocker

Authentic published v0.5.5 platform archive was downloaded and checked against its separately published checksums list. SHA256 is `33c7eccbfc42147743462328edf32d208c5426396155e32b5c9aa431e4bc289b`.

1. Its installer requires GLIBC_2.39 and cannot run on Ubuntu22.04.
2. On Ubuntu24.04 its installer runs but rejects **its own** archive: `Invalid artifact layout: missing .../immutable/bin`. Published archive has `bin/oqto-setup`, not `immutable/bin/oqto-setup`.
3. Its checksum argument consumes a single checksum entry, not the whole combined list; fixture derived the matching line from trusted published checksums before the layout rejection.

Evidence: `/tmp/oqto-previous-release/installer-help.log`, `installer-ubuntu24-help.log`, `/tmp/oqto-056-upgrade-proof.log`. No legacy bundle repacking/manual pointer seeding is used to invent a successful published-predecessor install. A real previously installed host/snapshot remains required for historical upgrade/data acceptance.

## CPU evidence — unresolved release decision

Exact official Pi1 x64 runtime tested through Ubuntu's qemu-user-static 6.2 package (downloaded through authenticated APT metadata; extracted privately, not host-installed). Reproducer: `scripts/dist/probe-pi-cpu.sh <qemu-x86_64-static> <pi-package-directory> <CPU-model>`.

- `Westmere`: version startup prints 1.0.0, exit0.
- `qemu64`: illegal instruction, exit132, repeated with bounded reusable probe.
- `Westmere,-ssse3`: illegal instruction, exit132.
- `Westmere,-sse4.2`: timeout; not classified as successful or conclusively as SIGILL.
- `Haswell`: version startup succeeds, but emulator warns some requested features are unimplemented; not a full Haswell fidelity proof.

Use private writable HOME/tmp and bounded termination. Bun startup requires writable scratch; a read-only scratch failure was diagnosed separately and is not counted as CPU evidence. These probes are emulated **version startup only**, not physical baseline-CPU/RPC/model acceptance. No older Pi or custom runtime was substituted. Generic qemu64 compatibility cannot be claimed. Need an explicit supported CPU-floor decision (Westmere/x86-64-v2 is a candidate for further qualification), or upstream official Pi1 baseline support plus physical-host tests.

## Still outstanding

Packaged authenticated full-shell chat/Files/selection, actual personal profile provisioning/service/sandbox acceptance, historical DB/session/config migration from a real previous installation, physical CPU/runtime qualification, source-authenticity signing/publication gates, and broader inherited guardrail/lint debt. Strict doctor accepted the fixture's unprovisioned auto-profile before deliberate invalid-config injection; do not substitute that for service/model readiness. No root/host services or real user homes were mutated.
