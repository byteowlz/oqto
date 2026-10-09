# Runner inventory is not Session placement

Status: accepted (2026-09-08). Tracked by `oqto-8x3p`; extends ADR-0024 without claiming its full enrollment/replay implementation.

## Decision

An administrator may configure additional runner targets under `backend.runner.targets`. Each target has an explicit ID, display label, Account-ID allowlist and mutual-TLS endpoint. Authenticated clients discover only targets granted to their Account through `GET /api/runner-targets`. An administrator role, SSH access or possession of a Herdr profile does not implicitly grant this API inventory. Transport credentials remain administrator-owned and sandbox-inaccessible.

This is **read-only inventory**, not an alternative PlacementStore. No target entry rebinds a Workspace, changes `ExecutionTarget`, mints a public Session ID, admits execution, migrates files or copies Pi histories. The existing personal Linux Workspace endpoint remains unchanged. `session_creation: false` is explicit in every response; online means a version-compatible capability response over authenticated transport, not that remote chat creation has been implemented.

Probes are Account-filtered before dialing, bounded to two seconds, concurrent across at most 32 configured targets, and coalesced for eight seconds per target. API responses omit transport addresses, credentials, grants and raw errors. Missing credentials/connectivity are unavailable; unsupported wire versions are incompatible. No Unix recovery/autostart fallback is used for these mutual-TLS targets.

The frontend consumes an optional host capability and displays a collapsible Machines roster, separate from WorkDirectories/Sessions. It polls every ten seconds, does not retain the roster across unmount/logout, hides stale success on API errors, and offers no misleading launch action. The same authenticated HTTP contract is usable headlessly; neither Herdr nor the frontend is the registration/control authority.

## Consequences and next seam

Machine grouping remains optional presentation, not a Session identity hierarchy. Future execution admission must bind a registered runner to an authorized Workspace Placement and resolve paths on that runner, rather than repointing the personal endpoint or treating WorkspaceLocation metadata as an execution route. Portable Session identity/history does not imply portable files, credentials or running processes. Before adding creation/attachment, prove Account/Placement admission, public/native ID mapping, oqto-log authority and reconnect/reload convergence.

The development Mac transport is loopback mutual TLS carried by an independently supervised SSH forward. This is deliberately static operator configuration, not a complete audited enrollment/rotation/revocation service. Dedicated certificate revocation currently means removing its trust/configuration and restarting the relevant service. Replacing certificates is an operator maintenance task; authentication never falls back to plaintext.

Read-only machine history is a separate, explicit single-Account grant (`history_read`), not execution admission. A dedicated runner can present an operator-selected existing oqto-log home without changing its process HOME or starting Pi. The preview uses stored public IDs, rejects ambiguous identities, and supplies no send/file/auth actions. It does not invent Session placement or imply live TUI attachment. See [existing machine history](../development/machine-history.md) for authorization, reload/disconnect behavior and proof.

Verification and reversible host setup: [Mac runner runbook](../development/macos-runner.md).
