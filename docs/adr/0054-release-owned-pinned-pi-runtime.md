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

## Consequences and verification

Qualification must exercise the exact extracted/packaged runtime, its assets, resolver precedence, private-home startup, canonical agent controls, configured model/chat, and clean install/update/rollback. An archive checksum or binary version alone does not prove authentication/model readiness, CPU compatibility, or a complete deployment. Do not publish until those gates pass. Runner-only targets and non-Linux platforms are not implicitly promoted to accepted full-release support.
