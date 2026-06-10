# Oqto canonical install layout

Status: draft reference for epic `oqto-vemr` child `oqto-vemr.1`.

This document defines the target host layout and file ownership model for install, deploy, and update.

Goals:
- One obvious location for shipped assets.
- One obvious location for admin/user mutable data.
- Deterministic update behavior (release switch + doctor), without ad-hoc copying.

## 1) Canonical host layers

| Layer | Purpose | Canonical host path(s) | Mutability |
|---|---|---|---|
| Release (immutable) | Versioned binaries + shipped defaults | `/var/lib/oqto/releases/<release-id>/` | Immutable after staging/activation |
| Active release pointer | Current running release | `/var/lib/oqto/releases/current -> <release-id>` | Symlink switch only |
| Stable executable entrypoints | Operator/user command paths | `/usr/local/bin/<bin> -> /var/lib/oqto/releases/current/bin/<bin>` | Symlink managed by activation |
| Admin config | Site policy/config managed by admins | `/etc/oqto/*` | Mutable |
| Runtime/service state | Databases, logs, queues, runtime data | `/var/lib/oqto/*` | Mutable |
| User/workspace state | User-local config/state and workspace materialization | `~/.pi/*`, `<workspace>/.pi/*`, `<workspace>/.oqto/*` | Mutable |

## 2) File classes and rules

Every shipped path must be assigned exactly one class.

| Class | Update behavior | Allowed location examples |
|---|---|---|
| `immutable_symlink` | Ship in release; expose via symlink only | binaries, shipped default templates, shipped unit templates |
| `mutable_copy_once` | Copy at first provisioning/instantiation; never overwrite silently | `AGENTS.md` baseline, workspace template outputs |
| `runtime_generated` | Never shipped; created by runtime/services | sessions, DBs, logs, caches, sockets |

Rules:
1. `immutable_symlink` files are owned by release activation, not by ad-hoc setup steps.
2. `mutable_copy_once` files may be seeded from shipped defaults, then user/admin-owned.
3. `runtime_generated` files are excluded from release artifacts and template payloads.

## 3) Copy vs symlink matrix

### 3.1 Platform-level Pi assets (`~/.pi/agent/*`)

| Path | Class | Action |
|---|---|---|
| Shipped extension defaults (`~/.pi/agent/extensions/<shipped>`) | `immutable_symlink` | Link from release defaults |
| Shipped skill defaults (`~/.pi/agent/skills/<shipped>`) | `immutable_symlink` | Link from release defaults |
| `~/.pi/agent/AGENTS.md` baseline | `mutable_copy_once` | Copy once (user may edit) |
| `~/.pi/agent/settings.json` | `mutable_copy_once` | Copy once (user may edit) |
| `~/.pi/agent/models.json` | `runtime_generated` | Generated/synced from EAVS/provisioning |
| `~/.pi/agent/sessions/*` | `runtime_generated` | Created by Pi runtime |

### 3.2 Workdir template materialization (`<workspace>/*`)

Workdir template outputs are always copied, never symlinked to immutable release assets.

| Path | Class | Action |
|---|---|---|
| `<workspace>/AGENTS.md` | `mutable_copy_once` | Copy from selected template |
| `<workspace>/.oqto/workspace.toml` | `mutable_copy_once` | Copy from selected template |
| `<workspace>/.pi/settings.json` | `mutable_copy_once` | Copy from selected template |
| `<workspace>/.pi/extensions/*` | `mutable_copy_once` | Copy from selected template |
| `<workspace>/.pi/skills/*` | `mutable_copy_once` | Copy from selected template |

## 4) Override precedence

For defaults that may be overridden, resolve in this order:

1. Workspace/user override (mutable)
2. Admin/site override (`/etc/oqto/...`)
3. Shipped default (immutable release)

This prevents updates from clobbering local customization.

## 5) Release artifact placement in repo (target)

Target repository structure for shipped assets:

```text
dist/
  manifest.toml
  immutable/
    bin/
    systemd/
    defaults/
      pi-agent/
      workdir-templates/
      templates/
  mutable-templates/
    etc/oqto/
    user/
```

- `dist/manifest.toml` is authoritative for class (`immutable_symlink`, `mutable_copy_once`, `runtime_generated`), ownership/mode, and install destination.
- Setup and deploy both consume this manifest.

## 6) Activation/update contract

All install/update paths should converge to this sequence:

1. Stage release into `/var/lib/oqto/releases/<release-id>/`.
2. Validate staged artifact (checksums, required files, permissions policy).
3. Atomically switch `/var/lib/oqto/releases/current`.
4. Refresh `/usr/local/bin/*` symlinks to `current/bin/*`.
5. Run doctor contract checks (post-activate; strict mode as gate where required).

No direct in-place replacement of shipped binaries outside release staging/activation.

## 7) Current drift vs target (as of now)

Observed current state:
- Transactional release deployment exists (`scripts/deploy.sh`) and already uses `/var/lib/oqto/releases` + `/usr/local/bin` relinks.
- Setup path (`setup.sh` + setup modules) still includes direct file installation/copy logic in several places.
- Shipped vs mutable assets are not yet centrally declared in a single manifest consumed by both setup and deploy.

Required follow-up tasks (mapped to epic children):
- `oqto-vemr.2`: introduce `dist/manifest.toml` + asset classification.
- `oqto-vemr.3`: unify setup and deploy activation path.
- `oqto-vemr.4`: apply template and `~/.pi` layering consistently.

## 8) Non-goals and guardrails

- No broad always-on privileged daemon for setup/update.
- Privileged operations remain narrow, explicit, and auditable.
- Runtime/user state is never shipped in release artifacts.
- Backward compatibility hacks should be minimized; migration should be explicit and finite.
