# Filesystem permission is one decision function, not a pile of lists

## Status

Proposed (2026-07-27). Supersedes the filesystem-permission surface of the
sandbox config (`SandboxProfile`). `oqto-mpn7` becomes the first implementation
of this model rather than a parallel effort. Related: ADR-0020 (pluggable
placement supervisor), ADR-0019 (container-per-Workspace).

## Context

`SandboxProfile` grew thirteen filesystem-related fields: `read_policy`,
`allow_read`, `deny_read`, `allow_write`, `deny_write`, `extra_ro_bind`,
`extra_rw_bind`, `overlay_enabled`, `overlay_root`, `overlay_paths`,
`scoped_paths`, `workspace_cache_enabled`, `workspace_cache_root`.

They do not compose. Precedence is implicit in the order the bwrap argument
builder emits mounts, so the effective access for a path cannot be read off the
config; it has to be derived by tracing code. Every filesystem defect found on
2026-07-26/27 was an interaction between two of these mechanisms rather than a
fault in any one of them:

- `extra_ro_bind` is applied after `deny_read`, so a workspace listing
  `~/.config/oqto` re-granted a directory the profile denied, exposing the
  backend signing secret.
- Under an allowlist, a `deny_read` of `~/.config` masked the allowed
  `~/.config/git`, silently removing git identity.
- A later fix to that skipped *every* home-relative deny, which silently
  dropped a workspace's own `deny_read` of a file inside its work directory.
- `extra_ro_bind` and `extra_rw_bind` were unioned on workspace merge, letting
  a workspace grant itself arbitrary read or write access to any host path.
- `scoped_paths` and `allow_write` both bind a writable path, with different
  missing-target semantics; the `warn` default silently discarded history for a
  new work directory.

Each was fixed individually. The shape of the defects is the finding: a model
where the answer to "can the agent read this path" depends on emission order
will keep producing them.

Two further forces:

**A work directory tree UI.** We want the user to select nodes in a work
directory tree and set what the agent may see, read, or write. That requires
answering "what is the effective access of this path" for arbitrary paths,
before anything is spawned, and showing *why*. Thirteen order-dependent lists
cannot answer that question.

**Backend portability.** ADR-0020 commits to a pluggable placement supervisor,
and external runtimes (for example NVIDIA OpenShell, which expresses filesystem
policy as declarative YAML) are candidate backends. `path -> access` is close to
the intersection of what bwrap, Landlock, macOS Seatbelt, and container policy
engines can all express. An ordered pile of overlapping mount mechanisms is not.

## Decision

### 1. Permission and materialisation are different concepts

Of the thirteen fields, five are not access control. `overlay_*` and
`workspace_cache_*` decide how a path is materialised and where writes land,
not whether a write is permitted. The remaining eight collapse into the access
policy below.

The five materialisation fields move to ADR-0020's typed mount descriptor:

- `overlay_*` becomes a mount materialisation mode (`bind` | `overlay` |
  `tmpfs`): the path reads from one source while writes land in another.
- `workspace_cache_*` currently combines two concerns. Its per-Workspace
  directory becomes a mount descriptor. Its tool-specific environment exports
  (`CARGO_HOME`, npm cache, Go cache, and similar) belong to process environment
  setup, not filesystem policy.

The processing order is **materialise, then authorise**. A mount descriptor says
what exists at a path; the access policy independently says what the agent may
do there. An overlay, cache volume, or bind mount grants no access by itself.

This division also survives placement changes. On container-per-Workspace
placements, copy-on-write and per-Workspace volumes are native runtime
facilities; on host placement, the adapter may implement the same descriptors
with bwrap overlays and host directories. Those mechanics must not leak back
into the permission language.

### 2. One primitive: an ordered rule set with a total resolution function

```
Access = none | read | write          // ordered; write implies read
Rule   = { path: PathPattern, access: Access, origin: LayerId }
Policy = { default: Access, rules: [Rule] }

Policy::access(path) -> Resolution { access: Access, winning_rule: Option<Rule> }
```

Resolution is **most-specific-match**: the rule whose path is the longest
prefix of the queried path wins. No match falls through to `default`. Equal
specificity resolves to the lower access (deny wins), so a tie can never
silently widen.

The existing surface maps onto it without loss:

| Today | Becomes |
|---|---|
| `deny_read` | rule with `access = none` |
| `allow_read`, `extra_ro_bind` | rule with `access = read` |
| `allow_write`, `extra_rw_bind` | rule with `access = write` |
| `read_policy = allowlist` | `default = none` |
| `read_policy = denylist` | `default = read` |
| `scoped_paths` | a rule whose path is templated |

Rules are not confined to the work directory. Agents need profile-governed
access to paths such as `~/.pi`, `~/.cargo`, system toolchains, caches, and
harness-owned state. Paths therefore use declared symbolic roots rather than
assuming `{workdir}` is universally applicable:

- `{workdir}` is the current work directory—the tree presented by the ordinary
  work-directory permission UI.
- `{home}` addresses profile-governed user state such as `~/.pi` and
  `~/.cargo`.
- `{resource:<id>}` references a named filesystem resource declared by a runner,
  harness adapter, placement, or administrator. Each declaration must identify
  its owner, resolution source, authority boundary, and whether the resource is
  available on the selected backend.

The neutral policy contains no Pi-specific `{session_shard}` concept. Pi's
adapter may declare `resource:harness_history` and resolve it to the current
work directory's Pi session directory. Another harness may resolve that resource
to a different path, expose no filesystem history resource, or keep history in
a non-filesystem store—in which case it does not participate in filesystem
policy. Likewise, a container or external runtime may mount the resource at a
physical path different from its host source. Policy governs the resolved
sandbox-visible path, not Pi's storage convention.

Resource identifiers are semantic contracts, not arbitrary strings. Their
schemas and owners must be registered; arbitrary environment-variable
interpolation is forbidden. The project term is **work directory**, not
`workspace`, for the directory in which a harness runs, so there is
intentionally no ambiguous `{workspace}` token.

Rule authority is independent of path location. System and administrator
profiles may govern any declared root. A work-directory tree UI normally emits
rules only below `{workdir}`; separate, deliberately designed UI sections may
expose agent state or caches. A work-directory or session layer cannot widen
access under `{home}` or any other root merely by naming it. Layer composition
still takes the minimum effective access, so a more-specific lower-authority
rule cannot override an administrator denial.

### 3. Resolution is pure, total, and explainable

`access(path)` takes no I/O, answers for any path, and returns the rule that
decided it. This is a hard requirement, not a convenience:

- the tree UI colours every node from it, live, with no spawn;
- "why is this readable" names a rule and the layer that contributed it;
- adapters and tests consume the same function, so what is displayed, what is
  tested, and what is enforced cannot drift apart.

### 4. Layers compose by narrowing only

Policy is assembled from layers (system defaults, admin profile, workspace,
per-session UI selection). A later layer may only reduce access:

```
for every path P:  access_after(P) <= access_before(P)
```

This is checkable rather than aspirational, and it generalises a rule we
enforced by hand, separately and inconsistently, for `allow_read`,
`extra_ro_bind` and `extra_rw_bind`. Widening is an administrator action that
replaces a lower layer, never an additive merge from a higher one.

### 5. Backends adapt; they do not reimplement

A backend adapter compiles a resolved `Policy` to its own mechanism: bwrap
mount arguments, Landlock rules, Seatbelt SBPL, or a container runtime's
declarative policy. Adapters must not carry their own precedence logic; if two
backends disagree about the effective access of a path, that is a bug in an
adapter, not a policy variant.

Each adapter declares the granularity it can express. Where a policy needs
something a backend cannot provide, the capability handling from `oqto-mpn7`
applies: every guarantee the backend *can* provide is still enforced, and the
gap is reported per `on_missing_capability` (`fail` | `warn` | `ignore`).

## Consequences

**Gained.** The effective access of any path is computable, explainable, and
testable without spawning a sandbox. The tree UI becomes possible. The
restrict-never-widen invariant becomes one checked property instead of several
hand-maintained merge rules. Adapters, including to an external runtime, have a
small surface to implement. Session-history scoping stops being a special
mechanism.

**Lost.** Most-specific-match is less expressive than ordered rules with
explicit precedence; "deny everything under X except Y unless Z" is no longer
expressible. That class of rule produced several of the defects above and
cannot be rendered in a tree UI, so the loss is accepted deliberately.

**Migration.** The eight permission fields are mechanically translatable, so
existing profiles and workspace configs convert without operator action. The
five materialisation fields migrate to typed mount descriptors; tool cache
environment exports migrate to process setup. The containment suite is the
acceptance test: it must pass unchanged against the new model, and then against
each adapter, which is what proves adapters agree.

**Open question.** Whether `list` (observe that a directory entry exists without
reading its contents) is a fourth access level. It is meaningful for the tree UI
and for hiding sibling work directories, but not every backend can express it.
Deferred until an adapter needs it; adding a level between `none` and `read` is
compatible with the ordering.
