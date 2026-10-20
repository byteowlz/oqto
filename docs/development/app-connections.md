# App Connection Host Bridge / SDK

Workstream `app-connection-bridge`, `oqto-3czz.2`; authority remains
[ADR-0058](../adr/0058-host-owned-credential-provisioning.md).

## Contract

SDK 0.4.0 adds negotiated `oqto-app/v3` and optional
`host.connections.invoke(slot, path): Promise<JsonValue>`. All older protocol
versions remain supported for their existing capabilities. Pre-v3 Connection
calls fail closed, never become standalone fetch, shell operations or env-based
credential injection. The Web mount does not advertise Connections to old Apps.

An App declares semantic slots, not vault keys or Connection IDs:

```toml
requested_capabilities = ["connection"]

[[capability.connection.slots]]
slot = "example-api"
expected_origin = "https://api.example.com"
profile = "https_get_v1"
```

This table belongs alongside the existing complete `oqto-app.toml` package
fields, not in a separate manifest. `connection` is the package request spelling;
`connections` is the canonical permission/SDK capability spelling.

```ts
const host = await connectOqtoApp();
if (!host.connections) throw new Error("Connections are not available on this mount");
const items = await host.connections.invoke("example-api", "/v1/items");
```

Use `connectOqtoApp` from `@byteowlz/oqto-app-sdk`. New App bundles require the
0.4.0 SDK distribution; old bundles do not acquire this API through a host update.
Manifest approval is not a binding: the Account must separately provision and
bind a credential through the protected host management path. No provisioning,
binding, unlock, credential/provider selection or raw-secret API exists in the
frame. No new credential management UI is implied by this wiring.

## Trusted path

1. `RuntimeOqtoAppFrame` obtains permissions for its own mounted Instance. Its
   parent captures Instance/work-directory identity; neither comes from the App.
2. SDK Host dispatch checks protocol/capability, rejects extra request fields,
   and validates bounded slot/origin-relative path shape.
3. `app-connections.ts` calls the authenticated parent facade in `lib/api/apps.ts`:
   `POST /api/apps/instances/<mounted-id>/connections/invoke` with parent-owned
   `workspace_path` plus the App's slot/path.
4. Existing backend `connections::invoke` derives Account and placement, rechecks
   live slot binding/grant/digest/revision, audits, and dispatches through the
   owning runner's existing credential HTTP gate. Provider byte attachment stays
   at that final trusted destination-bound boundary, never in the frame or env.
5. The frame receives bounded JSON only. Backend error bodies are not relayed;
   failures become safe structured SDK errors. Unsupported shared/remote
   placement still fails closed; backendless Device pairing is not Account
   credential authority.

Unmount/revocation fences pending permission lookups so they cannot create a
late orphan bridge. Suspension rejects pending/future local calls. An already
admitted bounded HTTP request may finish after revocation or disconnect; no
automatic replay/retry is introduced. The first profile remains HTTPS GET only.

## SDK provenance and build

The canonical source is the separate `byteowlz/oqto-app-sdk` repository, with
its ADR-0002 and CHANGELOG. The frontend consumes a hash-pinned **distribution
tarball** in `frontend/vendor/oqto-app-sdk`, not a sibling checkout or an
unpublished Git tag. `PROVENANCE.json` records source commit, version and SHA256;
the canonical Bun lock additionally records package integrity. Tests check both
installed version and archive hash. Do not hand-edit the distribution.

To update: run `pnpm check` and `pnpm pack` in the reviewed canonical SDK
checkout, copy its distribution tarball, update provenance, and regenerate the
frontend Bun lock with `bun install --lockfile-only --ignore-scripts`; prove
`bun install --frozen-lockfile --ignore-scripts`. A directory dependency is not
equivalent: Bun installs that local directory's devDependencies, introducing an
unrelated SDK development toolchain into the frontend. Use the packed artifact.

The source version is 0.4.0; no tag/npm publication or managed authoring-store
upgrade is claimed. Until separately released/provisioned, App authors can use
an explicit reviewed local SDK distribution when scaffolding. Existing managed
SDK-store pins must not advance to a nonexistent tag. The canonical frontend
installer/CI uses Bun; stale alternate npm/pnpm lockfiles are not rewritten here.

## Proof and remaining gates

- SDK `pnpm check`: format/lint/typecheck,97 tests, build/package/ESM contract gates.
- Frontend scoped Biome + Oxlint:0 warnings/errors.
- App suites24 tests: actual mount/MessagePort -> SDK Host -> authenticated facade
  with synthetic HTTP, forged-authority rejection, live denial, safe errors,
  old-version omission, pending-lookup unmount/revoke fences, archive integrity,
  existing sandbox/catalog/permission/theme/tab regressions.
- Frontend production build succeeds. This changes no appearance or layout.

Mocks are not real credential-use acceptance. Production kyz remains
experimental/unapproved, unchanged by this feature. Real authenticated
route-to-TLS/provider isolation, rotation/revocation/restart and no-leak acceptance,
CLI/native-MCP wiring and deployment remain separate gates in `oqto-3czz.2`.

Repository-wide frontend `tsc --noEmit` remains red in legacy/WIP surfaces,
tracked separately (`oqto-d0pc.1`); no type suppression is introduced. Preserve
that failed diagnostic evidence. A preserved-WIP baseline overlay using the same
installed SDK/toolchain and backend/Flash dependencies reports the same173
semantic diagnostic headers (quoted union order normalized), with none in touched
sources/tests. This proves no added adapter-source diagnostics, not a clean whole
frontend or an independent old-SDK comparison.
Accepted test/build debt is limited to the existing Vitest/Node26 loader
deprecation, uninitialized i18next in the legacy AppView smoke fixture and large
editor/mermaid/runner-target bundle chunks. Upgrading those harnesses or
repartitioning unrelated UI bundles is outside this credential adapter slice;
no warning limits/baselines are raised. They are not production kyz approval.
