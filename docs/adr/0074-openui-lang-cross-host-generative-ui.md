# OpenUI Lang as the cross-host generative UI payload

## Status

Accepted (2026-10-27, owner decision). Resolves the payload choice left open by
[ADR-0036](0036-interactive-output-parts-declarative-first.md) and fits the App
presentation model of ADR-0038. Supersedes the 2026-10-18 "do not adopt OpenUI"
adjudication. Tracked by epic `oqto-8qse`. Not implemented yet.

## Context

Agents need to answer with interactive UI (forms, tables, charts, cards, choices)
on every surface: Web (OqtoUI), native Pi TUI, oqt, GPUI desktop and mobile.
The A2UI v0.8 subset still present in the legacy backend/frontend is not
expressive enough and is not pursued further.

OpenUI (thesysdev/openui, MIT) provides OpenUI Lang: a compact, streaming-first,
interpreted UI language with a framework-independent core
(`@openuidev/lang-core`, 1.0.2 at decision time), component schemas that generate
model prompt instructions, local state and declarative query/mutation semantics.
It is constrained data/expressions, not arbitrary generated code; component
implementations are host-registered trusted code. Upstream ships React/Vue/Svelte/
Angular and React Native bindings, but no TUI, GPUI, SwiftUI or Compose renderers.
Its official Pi example is a Next.js demo using the hosted Gateway with no
authentication or tool approval; it is a reference only.

## Decision

1. **Payload.** OpenUI Lang is Oqto's generative/interactive UI language. A2UI
   is retired; remove its legacy code when the legacy frontend goes away.
2. **What we take.** Only `@openuidev/lang-core` (parser, validation, prompt
   generation) and, on Web, `@openuidev/react-lang` rendering primitives. Exact
   version pins, reviewed upgrades, vendored provenance like other pinned deps.
   `OPENUI_TELEMETRY_DISABLED=1` in every process; tests prove no telemetry
   egress. The OpenUI Gateway/cloud and upstream styled component libraries
   (`react-ui`) are not used.
3. **Oqto profile (owned by us).** A versioned component library definition:
   allowed components, props, actions, and a **required core set** every host
   must render natively (text, card, list, table, choice, confirm, text input,
   form, status). Optional components (charts, rich layouts) degrade to their
   text fallback where unsupported. The profile, not upstream's library, is what
   generates the model prompt. One profile, one set of design tokens from the
   Design Studio; every host renders the same components with the same look and
   feel. Shared conformance fixtures (OpenUI Lang input -> expected structure
   and visual reference) gate each renderer.
4. **Envelope.** Each interactive part travels in an Oqto envelope: stable part
   id, profile version, OpenUI Lang body, **required plain-text fallback**,
   answer schema, timeout, and originating Session/turn. Parts are durable in
   `oqto-log` like other canonical messages and reconcile by id, never by text
   or position (no second transcript authority; Pi still owns its JSONL).
5. **Actions and authority.** OpenUI query/mutation/tool actions never call
   tools, MCP clients or URLs directly. They become Oqto actions through the
   runner and the existing permission gate. Rendering or registering a component
   is not a grant. Answers are validated by the runner against the answer schema,
   not only in the client; ambiguous delivery is never auto-replayed. Peer or
   model-generated UI is untrusted content and cannot approve anything.
6. **Bounds.** Size, nesting, expression evaluation, refresh/query rates and
   resource access are bounded; violations fail closed to the text fallback.
7. **Hosts.**
   - Pi: our own `pi-openui` extension injects the profile prompt, marks parts
     for the runner, and renders the core set natively in the TUI (text fallback
     otherwise). Headless shows the fallback and reports unanswerable input.
   - Web: OqtoUI renderer on `react-lang` with Oqto components (first target).
   - oqt: uses the same extension and protocol.
   - GPUI desktop and mobile: native renderers implemented by their lane owners
     against the same profile, tokens and conformance fixtures.

## Consequences

- Models can emit rich UI in one language; every surface stays usable.
- We own four renderers (Web, TUI, GPUI, mobile); the profile and fixtures keep
  them consistent. Upstream churn is absorbed behind the profile version.
- A pre-1.0 language history means pins and conformance tests are mandatory.

## Verification required before calling it done

Same fixture renders on each host matching the shared visual spec; a real Pi turn
emits a form whose validated answer returns to the model and survives reload and
reconnect without duplicates; denied/revoked actions do nothing; oversized or
malformed input falls back to text; no telemetry or Gateway egress.
