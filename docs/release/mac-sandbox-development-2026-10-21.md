# Qualified standalone Mac sandbox development snapshot

Source checkpoint for the explicitly authorized standalone Mac update on
2026-10-21 (`oqto-v8n2.2`), not a public release or general feature acceptance.
The adjacent JSON manifest identifies the base commit and captured file hashes.
The source excludes concurrent policy-editor/permissions/frontend/oqt work.

Native Mac fmt and strict all-target Clippy pass. The sandbox suite has
**180 passing tests / 4 ignored**; two ignored helper entries are explicitly
exercised by their enclosing live tests. All seven live macos_launch cases pass.
Actual installed CLI checks establish:

- Positive Workspace write; denied read and write, with successful controls.
- Nested execution cwd does not add a new filesystem grant.
- Explicit profile overrides existing user config; unknown profiles reject
  before a harmless marker payload.
- Overlay/replacement-config conflict rejects before payload.
- Overlay, effective-report and config-inspection operations reject unsupported
  Mac backend before payload: these new inspection/overlay paths remain Linux-only.

Installed standalone SHA256:
`cc1d1b877f3b107eed7dbd61f8a27696c73a023e582783ac9742f142f91c8199`.
Only the known prior standalone binary was atomically replaced. Login resolution
and unchanged runner/listener/protected configuration were verified. No policy,
Runner/Pi process, ticket, grant or Herdr changes. Private prior binary/candidate
and detailed deployment receipts are retained off-repository; they must not be
published with credentials, tickets, native session data or config backups.

This artifact does not implement Mac overlays or enable isolated Herdr children.
Trusted parent-policy launch binding remains `oqto-v8n2.1`. Linux acceptance of the
profile-precedence correction remains tracked separately in `oqto-gss4.1`.
Existing proc-macro-error2 future-incompatibility debt is tracked in `oqto-aavh`;
strict gates did not suppress new warnings.
