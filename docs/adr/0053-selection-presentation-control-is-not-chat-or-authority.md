# Selection presentation controls are not Chat or authorization

Status: Accepted for the authenticated non-chat Web adapter; packaged deployment acceptance remains tracked by `oqto-4h1z` and `oqto-bp7p`. Real Pi1 serialized runner and authenticated gateway/native-renderer browser acceptance pass; physical TUI/GPUI mirroring and full-shell/deployment qualification are not claimed.

`pi-selection` provides versioned content-only Questions/Review and a private durable review store. Oqto renders that content with trusted first-party components and forwards bounded semantic controls through its authenticated Session routing and Runner, following ADR-0001/0024/0038/0041. We do not send these controls through the Chat composer/outbox, load agent-authored native code, embed an ad-hoc tailnet page, or create a second authority for Session history.

## Boundaries and alternatives

- Review IDs, answers, revision CAS and draft/submitted/cancelled states remain authoritative in the selection store. Oqto obtains them through scoped Runner controls; it does not read/write those files itself. `oqto-log` remains authoritative for Chat and its tool/interaction provenance. Transient presentation/status events are notifications, not a second durable answer store.
- Account authorization, the public Session-to-native identity mapping and mapped execution Principal are checked by the established routing/Gate boundary. Native scope is never substituted for `platform_id`. Presentation advertisements and renderer ACKs describe support/readiness only; neither authorizes operations or proves a human decision.
- Controls are allowlisted and command discovery must succeed before dispatch. Missing/unsupported controls fail explicitly instead of falling through to a model prompt. Correlation uses request IDs and stable review IDs, not text, list positions or visible order. Controls create no optimistic or durable Chat user bubble.
- Bindings are attached to the authenticated originating connection/Session and expire. There is no global last-client assumption. Disconnect, Session switch and reconnect invalidate stale callbacks; recovery fetches the same scoped durable review and uses revision CAS.
- A selected option, recommendation or submitted review never grants permission to perform a downstream destructive/privileged action. Runner grants remain necessary. Dedicated secure interfaces own password/secret entry.

An ordinary prompt/composer route was rejected because an absent slash command can become model input and its optimistic outbox can invent Chat messages. Direct browser/extension-store access was rejected because network reachability, paths and advertised metadata are not authorization. A separate bespoke HTML server was rejected for authenticated Oqto presentation; standalone Pi's protected browser fallback remains its independent supported surface.

## Acceptance checklist

Before accepting this decision/adapter: prove authenticated no-tailnet create/render/save/submit/get/reload; denied wrong Account/Session; exact native/public ID mapping; command absence/version rejection; no Chat ledger/outbox mutation; multiple connection bindings and expiry; stale revision and reconnect/switch cleanup; same durable answer after reload; keyboard/mobile/theme accessibility; and fresh generated types plus warning-free scoped Rust/frontend regression gates. Core extension tests and standalone Chromium do not establish Oqto adapter acceptance.
