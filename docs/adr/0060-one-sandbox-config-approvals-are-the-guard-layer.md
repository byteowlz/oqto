# One sandbox config: approval rules are oqto-sandbox's guard layer

## Status

Accepted (2026-10-15), owner decision. `oqto-g7rk.1`. Refines ADR-0059 (adaptive capability gates) and the oqto-sandbox configuration hierarchy (ADR-0051 lineage).

## Context

oqt (standalone Pi Durable coding) grew its own configuration: `~/.config/oqt/policy.toml` and `<workspace>/.oqt/policy.toml` for approval ceilings, beside oqto-sandbox's existing `~/.config/oqto/sandbox.toml`, `.oqto/sandbox.toml`, named profiles, tighten-only workspace merge and published schema. Two configs for one concern drift, confuse users, and would force Oqto and oqt to define workspace profiles twice.

## Decision

There is one sandbox configuration, owned by oqto-sandbox. Anything a consumer needs is added to oqto-sandbox; consumers do not invent parallel files.

- **Approval rules are the existing oqto-guard layer.** oqto-sandbox already documents a third security layer, *oqto-guard: runtime approval for gray-area paths*, configured as a profile's `guard` table (`paths`, per-path `policy` of `auto|prompt|deny`, `timeout_secs`, `default_on_timeout`). It was never implemented (the planned FUSE mechanism did not ship). oqt's guard implements runtime approval by executor replacement, so the `guard` table becomes the approval contract, extended additively: `outside` (policy for other outside paths, `prompt|deny`), `read`, `edit`, `shell`, `network` (`auto|prompt|deny`) and `auto_shell` (pre-approved simple commands). `paths` are requestable paths; `policy` keys are paths, optionally ending in `/*` or `/**` for a subtree.
- **Where it is set.** `[profiles.<name>.guard]`, a top-level `[guard]` in the user/admin file (overrides the profile: scalars replace, lists union, policy entries extend), and `[guard]` in the workspace file.
- **Merge rule.** The user/admin file may loosen or tighten its profile (it is owner-controlled). The workspace file (`.oqto/sandbox.toml`) is agent-reachable data and may only narrow: its guard forms a separate layer evaluated independently, the most restrictive answer wins, and any `auto` in it (policy entries, `auto_shell`, tool or network modes, `outside`) is rejected.
- **Fail closed.** Consumers that enforce approvals load the workspace file strictly: a file that exists but does not parse is an error, never silently dropped. The existing lenient `load_from_workspace` is kept for the runner until it is migrated; its fail-open behaviour is recorded as debt.
- **Containment vs approval.** Containment (what the executor can physically reach) and approval (what the operator is asked about, inside that) come from the same resolved config. Approval can never widen containment; containment denies are absolute.
- **Profiles are sandbox profiles.** Workspace kinds (e.g. private-normal, private-restricted, shared) are `[profiles.*]` entries carrying containment, permissions and, later, trusted providers; Oqto and oqt read the same definitions.
- **Decisions are not config.** "Always allow in this repo" choices are operator decisions held by the guard/runner outside every executor bind, never written into sandbox files the agent could reach.
- **`enabled`.** `enabled = false` disables runner sandboxing only. oqt always confines; the selected profile and permissions still apply.
- **Built-in hard denies** (credentials, system roots, protected runtime roots and anything containing them) stay in code above every configuration.

## Consequences

- oqt's `~/.config/oqt/policy.toml` and `.oqt/policy.toml` are removed without a compatibility shim (no users).
- `oq-permissions` consumes oqto-sandbox's typed guard layers; the permission carrier loads the sandbox config itself through oqto-sandbox.
- The sandbox JSON schema's `guard` table gains the new fields; the runner can adopt approvals later without a format change. `guard.enabled` remains the runner's switch.
- Follow-ups: derive oqt's executor containment from the resolved profile (today a strict floor), migrate the runner to the strict workspace loader, and move workspace-kind profiles and trusted providers into this schema.

### Behaviour changes

- `load_user_config` now fails on an unknown `profile` instead of silently
  falling back to defaults; a typo must not quietly change containment. The
  runner's workspace loader (`load_from_workspace`) stays lenient (warn + none)
  until the runner moves to `try_load_from_workspace`.
- oqt ignores `enabled`: it always confines. `enabled` remains the runner's switch.
- Guard path keys accept plain paths with an optional `/*` or `/**` subtree
  suffix; any other glob fails closed.
