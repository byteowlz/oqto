# Canonical installation layout and file-class model

Status: accepted (formalizes the closed task oqto-vemr.1; implementation tracked under epic oqto-vemr)

Install, deploy, and update converge on one host layout, one authoritative shipped-asset manifest, and one activation sequence — so there is exactly one place for shipped assets, one place for mutable data, and deterministic updates (release switch + doctor) with no ad-hoc copying.

## Host layers

| Layer | Purpose | Canonical path(s) | Mutability |
|---|---|---|---|
| Release (immutable) | Versioned binaries + shipped defaults | `/var/lib/oqto/releases/<release-id>/` | Immutable after staging |
| Active release pointer | Current running release | `/var/lib/oqto/releases/current -> <release-id>` | Symlink switch only |
| Executable entrypoints | Operator/user command paths | `/usr/local/bin/<bin> -> .../current/bin/<bin>` | Symlink managed by activation |
| Admin config | Site policy/config | `/etc/oqto/*` | Mutable |
| Runtime/service state | DBs, logs, queues, sockets | `/var/lib/oqto/*` | Mutable |
| User/workspace state | Per-user + workspace materialization | `~/.pi/*`, `<workspace>/.pi/*`, `<workspace>/.oqto/*` | Mutable |

## File classes

Every shipped path is assigned exactly one class, declared in `dist/manifest.toml` (authoritative for class, ownership/mode, install destination, and method). Setup and deploy both consume this manifest.

| Class | Update behavior |
|---|---|
| `immutable_symlink` | Release-owned immutable asset; installed via the manifest's `install_method` (symlink where practical, copy where the consumer requires a real file — e.g. shipped Pi extensions use `copy`). |
| `mutable_copy_once` | Seeded from a shipped default at first provisioning; thereafter user/admin-owned, never silently overwritten. |
| `runtime_generated` | Never shipped; created by runtime/services. Excluded from release artifacts and template payloads. |

## Copy vs symlink

- **Platform Pi assets (`~/.pi/agent/*`)**: shipped extension/skill defaults are `immutable_symlink` (release-owned); `AGENTS.md` and `settings.json` baselines are `mutable_copy_once`; `models.json` and `sessions/*` are `runtime_generated`.
- **Workdir templates (`<workspace>/*`)**: always copied from the selected template (`mutable_copy_once`), never symlinked to immutable release assets — so workspaces are independently editable.

## Override precedence

For overridable defaults: (1) workspace/user override → (2) admin/site `/etc/oqto/...` → (3) shipped default. Updates never clobber local customization.

## Repo placement of shipped assets

```text
dist/
  manifest.toml            # authoritative asset map
  immutable/
    bin/  systemd/  seccomp/
    defaults/  (pi-agent/  workdir-templates/  onboarding-templates/  templates/)
  mutable-templates/
    etc/oqto/  user/
```

(Pi extensions under `defaults/pi-agent/extensions/` are pulled from the source repo at release time and not tracked in git; the manifest + fail-closed packaging enforce their presence — see the dist tooling.)

## Activation / update contract

All install/update paths converge on: (1) stage release into `/var/lib/oqto/releases/<release-id>/`; (2) validate the staged artifact (checksums, required files, manifest contract); (3) atomically switch `current`; (4) refresh `/usr/local/bin/*`; (5) run doctor contract checks (strict where gated). No in-place replacement of shipped binaries outside release staging/activation. This activation/symlink-switch model is the same one ADR-0002 relies on for socket-activated runner units.

## Non-goals

- No broad always-on privileged daemon for setup/update; privileged operations stay narrow, explicit, auditable (consistent with the `oqto-hostd` broker, ADR-0009).
- Runtime/user state is never shipped in release artifacts.
- No backward-compatibility hacks; migrations are explicit and finite.

## Consequences

- Remaining convergence is tracked under epic `oqto-vemr` (`.2` dist manifest + classification, `.3` unify setup/deploy activation, `.4` template + `~/.pi` layering). The dist manifest, packaging, and fail-closed validation already exist.
- The current install contract snapshot (per-profile paths/services/checks) lives at `docs/reference/install-contract.md`, generated from the typed manifest in `oqto-provisioning` — reference, not a decision.
