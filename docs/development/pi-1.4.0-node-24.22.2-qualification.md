# Pi 1.4.0 / Node 24.22.2 release qualification

Date: 2026-10-30. Tracking: `oqto-w0md`, `oqto-bq0v`.
Scope: Linux x86_64 full-release Node/npm runtime. Preparation and packaging only;
no live release link, profile, credentials, Session or service was changed.

## Failure and prevention

The user's deployment reached `oqto-setup check-release` with an old staged Pi 1.1.0
receipt while the release manifest declared 1.3.0. The full validator correctly
refused it. Reproducing staging in the current tree rejects that stale receipt early.

An additional red regression showed that a cached receipt matching the release
manifest could still hide a conflicting dependency/package/lock pin. Staging now
checks all three declarations, exact npm integrity, Node archive digest and lock hash
before acquisition or reuse. Receipt kind is checked too. No validation was weakened.

A second red regression found retired extension directories surviving repeat
`dist-sync`: the obsolete pi-oqto-todos and replacement pi-todolist both registered
Todo during native combined autoload. Build-owned extension staging now prevalidates
the source set and recreates the exact selected closure. This does NOT reconcile or
remove user-owned extensions; installed user migration remains the existing managed
supersession mechanism. Dirty sibling source checkouts remain untouched.

## Pins

- Latest stable Pi verified from npm metadata: **1.4.0**.
- Pi npm integrity:
  `sha512-Z41LpZ3yYHSC/UUps6Mr39UMFPwH8elhJBG9DY1P7kuzsE5qSK/6eMj2EHzO5evJRQWLbKSqUMw+4nCNtIyiQg==`.
- npm closure lock SHA-256:
  `0649819888b9e67a4802556bd7a7bacb81c14aeb68580ac49e222837869e322e`.
- Node stays on the accepted **24 LTS** line, advanced from 24.14.1 to **24.22.2**
  (latest 24.x in the official release index at qualification).
- Official Linux x64 Node archive SHA-256:
  `48cc0e21e048790e54c0cf5749c2725005313d652651c40b7a21feb9697aaf75`.
- Standalone Pi x64/arm64 archive pins also match official v1.4.0 SHA256SUMS;
  standalone CPU compatibility is NOT established by updating those checksums.

`dependencies.toml`, `dist/manifest.toml`, the runtime package declaration and npm
lock were updated together. All 121 installed npm packages came from the integrity-
locked closure using isolated npm config and `--ignore-scripts`. Node acquisition
verified its archive checksum before extraction.

## Evidence

- `scripts/release/pi-update.py --prepare`: immutable tracked source candidate,
  official release checksums and exact npm metadata/lock.
- Real Pi 1.4.0 RPC qualification: passed, evidence under
  `/var/tmp/oqto-pi-1.4.0-qualify-20261030/rpc-evidence`.
- Real bundled Pi + sandboxed Runner/provider/cold-restore proof: passed under
  `/var/tmp/oqto-pi-1.4.0-qualify-20261030/sandbox-evidence`.
- All 14 shipped extension factories/session starts AND combined catalog: passed
  with actual Pi 1.4.0 and Node 24.22.2 after exact closure staging.
- Managed extension supersession proof: passed; native Todo/Selection/history tools,
  synthetic provider turn, same native identity on cold restore and preservation of
  unknown personal/project/package extensions.
- Offline update discovery tests: 9 passed; runtime lock/acquisition tests: 3 passed;
  staging idempotence/drift tests, exact extension closure and source-pin preservation
  tests passed. Release-assets tests: 4 passed.
- `just dist-stage-pi-runtime` confirms the staged receipt matches current pins.
- Actual `just dist-package` completed, including typed full-release validation,
  tar placeholder checks and checksum verification. Artifact:
  `dist/out/oqto-0.5.11-dev.20261030132931.g04f41b6b.dirty-x86_64-unknown-linux-gnu.tar.gz`.

The artifact is explicitly recorded as a dirty development build. This is not a
published release or evidence that any deployment was activated. Physical baseline
CPU emulation was not rerun (no qemu-x86_64 executable in this lane); the standalone
binary SIGILL qualification issue is separate from the official Node/npm lane.

Backups outside the release payload preserve the old stage at
`/var/tmp/oqto-pi-1.4.0-qualify-20261030/previous-staged-pi-1.1.0` and the old extension
stage at `previous-staged-extensions`. No backup directory is shipped in immutable
release assets. The initial Pi 1.4.0/old-Node candidate is retained separately;
`runtime-node24.22.2` is the qualified combined candidate.

Logs: `/tmp/pi-1.4-{rpc,sandbox,shipped-extensions-clean,extensions-proof,package,
regression-gates,release-assets}.log`.
