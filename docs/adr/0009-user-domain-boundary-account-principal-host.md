# The "user" domain splits into Account, Principal, and host primitives

The word "user" smeared three distinct concepts across `oqto/src/user`, `oqto/src/user_plane`, `oqto-host`, and `oqto-usermgr`. We separate them by concept and rename for clarity, resolving the ambiguity in the glossary (Account vs Principal).

## Resolved boundary

| Concept | Crate | Scope |
|---|---|---|
| Platform **Account** | `oqto-accounts` (renamed from planned `oqto-users`, 3ct7.5) | identity, roles, auth support, API keys |
| OS **Principal** lifecycle | `oqto-hostd` (renamed from `oqto-usermgr`) + `oqto-provisioning` | privileged broker that applies validated host mutations + the typed contract describing target host state |
| Host primitives | `oqto-host` (kept, shrunk) | `linux_users`, `sandbox`, `process` as a pure library; `runtime.rs` deleted (ADR-0001) |
| User-data plane | *(dissolved)* | `user_plane/direct.rs` and the `UserPlane` trait deleted; only the runner-mediated path survives, as part of the runner-client seam |

## Why oqto-host survives but oqto-hostd is the broker

`oqto-host` stays a library because the **runner** must link its `sandbox`/`process` primitives per session without pulling in any privileged daemon. `oqto-hostd` is the privileged daemon that *wields* those primitives across the privilege boundary on behalf of the unprivileged backend — and is scoped to become THE privileged host-mutation broker, not just user creation. Several scattered privilege-escalation paths fold into it: socket-dir/unit setup (ADR-0002), sudoers remediation (oqto-vemr.6), privileged runner restart (oqto-m94p). This consolidates the privilege surface that has repeatedly produced visudo bugs into one audited broker.

Layering for concept B: `oqto-provisioning` (plan / typed contract) -> `oqto-hostd` (execute, validated) -> `oqto-host` (how / primitives).

## Consequences

- Glossary: code, docs, and APIs use **Account** for platform identity and **Principal** for the OS uid; "user" is avoided where either is meant.
- The `user_plane` dissolution is a delete-and-collapse step (ADR-0008): once `direct.rs` is gone there is one impl, so the trait collapses into the runner client.
