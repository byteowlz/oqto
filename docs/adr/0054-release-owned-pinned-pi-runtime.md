# ADR-0054: Release-owned pinned Pi runtime

Status: Accepted by explicit user direction; packaging/install acceptance tracked by `oqto-bp7p`.

## Context

Easy deployment must pin and ship official Pi 1.0.0. A new runner must not accidentally execute an older user/global Pi, and platform rollback must restore its matching runtime without rewriting user-owned native Pi installations.

## Decision

- Full Linux x86_64 bundles contain the official checksum-qualified 1.0.0 archive and its extracted package under `immutable/pi-runtime/`. Keep required theme assets, not only the executable.
- The runner's default runtime resolver first locates that immutable package relative to its actual executable. Release-owned runner and Pi therefore move and roll back together. Explicit runner configuration remains a separate validated configuration surface; user-owned native Pi files are not rewritten by this default.
- Existing rootless/system runtime channels remain available when running outside an immutable full release. They do not outrank a runtime shipped beside a release runner.
- Runtime staging checks the pinned official archive hash, version and hermetic RPC response. Installation/runtime helper uses the same verified archive engine, with packaged archives preferred on its default acquisition route.
- Capability advertisements do not grant execution or presentation permissions. No selection/history authority changes.

## Node/npm qualification candidate (2026-10-03)

User direction is to qualify the official Pi1 npm/Node launch path before narrowing CPU support based on the standalone Bun executable. The Node candidate keeps Pi exactly 1.0.0, pins official Node24.14.1 by archive SHA256, and locks the official npm closure with SHA512 integrity for every fetched package. Install lifecycle scripts are disabled. Its launcher resolves the release-owned Node beside itself, never a Node from user PATH.

An explicitly named new runtime tree can be prepared with `just dist-stage-pi-node-runtime NEW_DIRECTORY`; it refuses existing destinations and never promotes user/runtime links. See `docs/release/pi1-node-qualification.md` for measured CPU/RPC/runner evidence and independent Bun CLI limits. This does **not yet switch** the full-release workflow or the standalone archive acquisition engine: that integration must carry matching checksum semantics, the same immutable runtime identity and the installation/update/rollback gates. Existing Bun development artifacts remain identifiable and unchanged.

## Combined Node canary implementation (2026-10-06)

ADR-0055 supersedes the permanent1.0.0 freeze with latest-qualified-stable policy. The isolated Linux0.5.7 candidate switches full-release staging to exact official Pi1.0.3/npm + separately pinned Node24.14.1, without live promotion. The implemented manifest mode is `bundled-official-node-npm`; unsupported runtime modes fail the shared typed activation/doctor contract. Exact lock/integrity/version receipts move with runner and assets. Official contained Node CLI aliases become adjacent-Node launchers because canonical release extraction deliberately rejects symlinks; original JS target locations and the entire closure remain intact. See [canary evidence and limits](../release/linux-node-canary.md). This is implementation on a candidate branch, not release/install acceptance or a change to user-owned native Pi.

## Consequences and verification

Qualification must exercise the exact extracted/packaged runtime, its assets, resolver precedence, private-home startup, canonical agent controls, configured model/chat, and clean install/update/rollback. An archive checksum or binary version alone does not prove authentication/model readiness, CPU compatibility, or a complete deployment. Do not publish until those gates pass. Runner-only targets and non-Linux platforms are not implicitly promoted to accepted full-release support.
