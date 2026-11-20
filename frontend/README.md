# Oqto web frontend

Vite and React frontend for Oqto's authenticated session chat, workspaces, files,
Apps and terminal attachments. The legacy shell and OqtoUI consume the same
platform contracts; Runner-owned durable history remains authoritative.

## Development

From the repository root:

```bash
cd frontend
bun install --frozen-lockfile
cd ..
just dev
```

Vite listens on port 3000 and proxies `/api` and WebSocket traffic to the configured
backend. In `frontend/.env.local`, set only the endpoints needed for the selected
development environment:

```bash
VITE_CONTROL_PLANE_URL=http://127.0.0.1:8080
VITE_CADDY_BASE_URL=http://127.0.0.1
VITE_FILE_SERVER_URL=http://127.0.0.1:41821
```

Keep credentials and real host configuration outside Git. Development UI actions
are real backend actions, not a read-only preview: use an isolated test backend,
Runner and workspaces by default. Do not expose an unrestricted Vite listener.
Tailnet previews need authenticated HTTPS and HMR/WebSocket proxying.

## Checks and packaging

```bash
cd frontend
bun run test:run
bun run lint
bun run build
```

`bun run lint` is read-only. Biome and Oxlint receive every tracked frontend
file from the Git index in bounded argument batches, preserving their existing
tool policies; no new source exclusions are added. Ignored staged runtimes and
untracked scratch are not directory-walk inputs. Add new source to the index
before checking it. Missing tracked files and tool failures fail the gate.
The two provider-icon JSON metadata files retain their canonical, hash-bound
bytes: Biome still parses/lints them, but does not reformat them. Project `just lint`
also runs the offline `just check-provider-icons` source/export integrity gate.
Tracked generated TypeScript is checked, not silently rewritten; the explicit
maintenance command remains `bun run format:generated-types`. Hook, OqtoUI type
and architecture checks still run after source lint. Flash adapters consume the
public `platform/flash-runtime-contract.ts` in-process interface: the Flash host
injects its native worker factory. Public builds and synthetic adapter tests do
not import a private Flash checkout or its SDK. Native history, target identity,
transport and credential persistence remain host-owned; no server or in-memory
fallback is introduced. Shared read-only OpenUI rendering is host-neutral and
retains the same strict view checks after extraction; its pure decoder accepts
raw tool data, but no browser state, transport or actions. Draft persistence and
browser speech are separate platform responsibilities.

Use [the canonical packaging sequence](../docs/reference/build-and-deploy.md) for
deployments, not an ad-hoc copy of `frontend/dist`. Vendored runtime dependencies
and archives are part of the reproducible build input, with licences/provenance.
Normal builds must not require sibling development checkouts.

## Browser speech

Local recognition keeps microphone audio in the browser and does not silently
fall back to a remote service. Model preparation, cancellation and unsupported
runtime conditions must be visible. Local recognition requires HTTPS/localhost
and cross-origin isolation. Vite isolation is opt-in:

```bash
VITE_LOCAL_SPEECH_ISOLATION=1 bun run dev
```

Review COOP/COEP compatibility with external frames and opener flows before using
those headers in production. Speech runtime assets are staged by the package's
sync scripts; preserve their upstream licences and pinned provenance.
