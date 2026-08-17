# OqtoUI zero-baseline guardrails

OqtoUI lives under `frontend/src/oqto-ui/` and follows ADR-0037/0038. It does not extend Workbench or legacy frontend state.

```text
app/       composition and authenticated routes
layout/    View identity, layout schema, docking/tabs, fidelity
sessions/  Session catalog and navigation
chat/      canonical Session timeline and composer
files/     file projection and Files View
gallery/   Gallery App vertical slice
platform/  live HTTP/event/storage adapter and shared contracts
dev/       development-only scripted adapter and traces
```

## Dependency rules

- `app` composes features.
- A feature imports only itself and `platform`; it never reaches another feature's internal state.
- `layout` imports only `layout` and `platform`.
- `platform` imports no product feature. Browser/network/storage APIs and bounded `unknown` narrowing live only here.
- `dev` imports only `dev` and the `platform` interface. Production code must not import `dev`.
- No OqtoUI source imports legacy Chat, Sessions, App registry, contexts, sockets, or Workbench.
- State models are framework-independent; React Views subscribe through narrow interfaces.

## Mechanical rules

`bun run lint:oqto-ui-guardrails` enforces the zero baseline, including dependency direction/cycles, no `any`, bounded interfaces and source files, no raw `useEffect`, no custom DOM-event coordination, adapter-only browser APIs, translated user-facing text, semantic-token-only styling, no feature radius/shadow values, and no broad barrels/imports.

Exceptions require an exact file/rule entry in `frontend/scripts/oqto-ui-guardrail-exceptions.json`, owner approval, a tracker issue, rationale, and removal condition. Stale exceptions fail.

The interface checklist in `docs/frontend/oqto-ui-interface-checklist.md` remains the review gate for behavior that cannot be proved statically.
