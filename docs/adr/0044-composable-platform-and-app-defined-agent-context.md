# Composable platform and App-defined Agent Context

Oqto needs Agents to understand references such as “these images,” “this slide,” “the selected clips,” or “the current record” without scraping a View, guessing from visible order, or requiring Oqto to predefine every future App domain. We will use one host-neutral Agent Context substrate assembled from independently authoritative Context Providers: Oqto platform and compositor providers, reviewed first-party View providers, optional external providers such as `ctx`, and immutable App Definition-declared providers. Oqto owns identity, authorization, validation, revisions, reconnect, revocation, disclosure, and agent delivery; an App owns only its domain topic schemas, values, opaque domain-reference meanings, and semantic actions.

**Status:** accepted

## Decision

### One provider registry, multiple authorities

A queryable Context Provider Registry describes providers available to the authenticated requester and Work Session. Provider identity is stable and names its authority:

- `oqto.platform` owns Account, Workspace, work-directory, Session, current-turn, attachment, resource, and capability facts;
- `oqto.compositor` owns requester-local Arrangement, Container, Container Content, visibility, focus, and Presentation Context facts under ADR-0041 through ADR-0043;
- reviewed first-party Views such as Files, Gallery, and Chat own their semantic interaction state while remaining platform code;
- `app-instance:<instance-id>` owns topics declared by that Instance's pinned Definition;
- an optional `desktop.ctx` adapter exposes bounded current-desktop metadata and explicit Bundle availability from the separate `ctx` product.

Providers refine rather than overwrite one another. A broad `ctx` observation that Oqto is active, exact Oqto knowledge of the active App Instance, and that App's selected domain objects may all be present with distinct provenance and precision.

No provider may publish under another provider's namespace. In particular, generated App code cannot assert platform, compositor, Account, grant, resource-authority, or Session facts.

### Oqto standardizes the envelope, not all domain vocabulary

The platform defines a versioned, serializable envelope containing at least provider identity, topic ID, topic-schema version, context revision, update time, availability, disclosure class, truncation/recovery metadata, and a bounded JSON value. Rust remains canonical for the envelope and exported values use typed `JsonValue`; arbitrary executable values and `any` are forbidden.

The platform offers the same semantic operations across authorized hosts:

```text
context.catalog(provider?)
context.get(provider, topic, at_revision?)
context.watch(provider, topics, from_revision)
actions.catalog(provider?)
actions.invoke(provider, action, expected_context_revision, input)
```

React stores, browser events, DOM state, Pi custom messages, native callbacks, and `ctx` files are adapters. They are not the contract. Pi/OqtoUI, future GPUI/native clients, and headless Agents consume the same catalog, snapshots, revisions, and deterministic traces.

Oqto may publish optional conventions and SDK helpers for recurring concepts such as current item, selection, focus, viewport, and diagnostics. They remain conventions until repeated real Apps justify promotion; they are not a closed platform enumeration.

### App Definitions declare context as immutable data

An App may request an `agent_context` capability and include a context catalog, JSON Schemas, descriptions, disclosure/lifetime declarations, resolver relationships, and semantic action declarations in its `.oqtoapp` package. These files are validated, bounded, and included in the immutable Definition digest. Publication does not grant the capability. A grant is scoped to the exact Instance, Definition digest, topic/action set, binding, subject, and requester policy.

The sandboxed App may publish a current value only for a declared topic through its nonce-bound MessagePort Bridge. The Host validates it against the pinned schema; the backend assigns the authoritative monotonic revision and records only the retention class the topic declares. App code cannot choose platform revisions, write directly to the Context Store, communicate directly with an Agent, or turn a context update into a model message.

Catalog descriptions and context values are untrusted data. They never become system or developer instructions. Arbitrary HTML, JavaScript, prompt fragments, tool definitions, or executable schemas are rejected. JSON Schema support is an explicitly versioned, bounded subset; unsupported keywords fail publication rather than being ignored.

### Context is reference-oriented and distinct from durable data

Context normally contains opaque references, small labels, counts, ranges, and bounded previews. Durable document/business state remains in files, bound resources, or an App's semantic operation domain. Large datasets are queried through authorized resolvers with pagination and explicit truncation; they are never pushed into a model because they are visible in a View.

Open, visible, focused, selected, and query-result are distinct states. A Gallery selection cannot be inferred from virtualized/visible order. Oqto validates platform resource references and their grants; an App interprets its own opaque domain references and resolves them only through pinned, authorized operations.

Context publication does not transfer ownership or authority. Moving App Content between Containers, copying an App, changing an Arrangement, or focusing a View does not copy context grants or App Instance data.

### Lifetimes and disclosure are explicit

Every topic declares one lifetime:

- `ephemeral`: cursor, hover, playhead, viewport, active tool; retained only as needed to serve the current connection and normally fetched on demand;
- `session_local`: explicit selection, focus, current slide/record, comparison set, active filter; recoverable for the owning requester/Work Session within a bounded retention policy;
- `durable_reference`: a reference to durable data, not a second copy of that data.

Every topic declares a disclosure class such as ambient metadata, explicit user intent, sensitive, or high-volume. Catalog and availability may be less sensitive than values. Policy can therefore reveal “three items are selected” while requiring explicit authorized retrieval of their content.

Context is pull-by-default. Publishing, changing, focusing, or selecting never wakes an LLM, injects a Chat turn, or resolves an unrelated pending tool. A running authorized Agent workflow may subscribe, but event delivery remains typed and does not itself request inference. Sensitive topics may require per-use user activation in addition to the standing grant.

### Explicit user intent is represented by Context Attachments

When a user deliberately chooses “Add to Chat,” drags a semantic selection onto the composer, or uses an equivalent host-owned gesture, Oqto creates a visible **Context Attachment** for the pending user turn. This is distinct from ambient availability: it records that the user intends one provider topic at one revision to accompany that turn.

A Context Attachment is a structured Chat part in oqto-log, not text pasted into the prompt. It records an attachment identity, provider, topic, schema version, proposed/pinned revision, human label, provenance, and resolution policy. Adding the composer chip captures the proposed revision; sending atomically pins an authorized snapshot or durable reference. If the topic changed before send, the Host asks whether to attach the new revision, preserve the old retained revision, or remove it. If the requested revision is unavailable, sending fails visibly; Oqto never silently substitutes current state.

The Host owns attachment UI and composer mutation. Apps may initially only publish declared topics; host-owned Container/View chrome offers attachment. A future App Bridge operation may request that the Host present an attachment affordance, but it cannot silently attach data, submit a message, wake an Agent, choose iframe security policy, or bypass user review.

Model projection is context-budgeted. By default, the Agent receives only a compact, untrusted description and an opaque scoped handle—not the complete topic value, catalog, schema, or referenced dataset. Small policy-approved values may use a bounded compact projection. Full values are exceptional and explicit. A durable oqto-log part never stores a reusable bearer credential: the Runner remints short-lived access handles after authorization checks and binds them to the Account, Work Session, Chat turn, provider, topic, revision, permitted resolution operations, and retention policy.

An attachment is user-selected data, not an instruction channel. App-authored labels, descriptions, values, and schemas remain untrusted content and cannot alter system/developer instructions or define executable Agent tools.

### Agent access is context-efficient and host-neutral

The canonical authority remains the Context Provider Registry and Runner Capability Broker, not Pi, a CLI, code mode, or MCP. Agents access it through a small semantic contract for discovery, description, retrieval, reference resolution, bounded waiting, and revision-bound action invocation.

The primary Pi integration is an operator-owned adapter plus a scriptable `oqto context` CLI. Explicit attachments are addressable through commands conceptually equivalent to:

```text
oqto context attachments
oqto context attachment describe <handle>
oqto context attachment query <handle> ...
oqto context attachment resolve <handle> ...
oqto context act --provider ... --action ... --expected-revision ...
```

The CLI's stable JSON output is a headless fallback and a code-mode substrate. Discovery, schema inspection, pagination, filtering, projection, aggregation, and bounded waits execute outside the model context; only the Agent's selected finite JSON result enters the conversation. Intermediate catalogs, records, schemas, and watch events are not replayed into the model. Queries enforce time, memory, byte, call-count, pagination, and output limits and expose no ambient credentials, internal stores, DOM, iframe MessagePorts, arbitrary network access, or authority-bearing identity flags.

Read-only querying and mutation remain separate. Code-mode queries may prepare an action request but cannot hide a mutation inside arbitrary query code. Semantic action invocation stays explicit, audit-visible, grant-checked, and bound to expected topic revisions.

MCP may later project the same operations for external clients, but it is not the canonical interface or an internal authority. An MCP adapter must remain stateless with respect to context truth and delegate identity, authorization, revisions, grants, and execution to the Broker. Pi does not require an MCP process merely to gain code-mode behavior.

### Reads and mutations are separate

Context describes state; it is not a writable control object. Agents mutate an App only through cataloged semantic actions. Each action declares a bounded JSON input schema, required topics/resources/grants, user-activation policy, and a pinned implementation or reviewed platform handler. Invocation includes the context revision the Agent acted upon. If relevant state changed, the Gate returns a typed stale-context conflict and performs no mutation.

Actions pass through the Runner Capability Broker and Gate under ADR-0038. The Gate rechecks acting identity, Work Session, provider/Instance, Definition digest, grant, binding, expected revision, and target Principal at execution time. UI placement and visibility never confer server authority.

### Reconnect, gaps, and revocation fail closed

Each provider stream has monotonic revisions and resumable watches. A subscriber supplies its last accepted revision. The provider either resumes contiguously or emits an explicit gap requiring a fresh bounded snapshot. Clients never reconcile by text, array index, visible order, or item count.

Revocation, suspension, uninstall, Definition change, binding-owner deletion, Account/Workspace authorization loss, or Work Session end invalidates affected handles immediately at the Gate and emits a typed lifecycle event to connected Hosts. Hosts stop subscriptions and reject pending and future operations. Reconnect reauthorizes from durable grants; browser state cannot resurrect access.

Session-local context retention is requester-scoped. App Copy and Installation promotion transfer no context values, subscriptions, grants, selections, or presentation state under ADR-0038.

### `ctx` is complementary, not absorbed

`ctx` continues to own cross-application desktop capture: active app/window, browser URL and Page Text, accessibility Selection Capture, Screenshot Capture, clipboard, narration, Capture Sessions, Sections, Captures, and reviewable Bundles. These are Best-Effort observations or explicit durable evidence. Oqto does not reimplement their platform capture paths, and `ctx` does not become Oqto's authorization or live semantic state authority.

An optional adapter may map lightweight `ctx current` state into `desktop.ctx` topics. Page Text, screenshots, clipboard, Selection Captures, narration, and Bundle contents require explicit retrieval or attachment; they are never ambient prompt material. Conversely, an explicit user action may capture a bounded Oqto semantic snapshot and resource previews into a `ctx` Bundle with provenance and revisions. Merely existing as transient Oqto context does not make state a `ctx` Capture.

`ctx` works without Oqto, and headless Oqto works without `ctx`. Desktop observation remains read-only evidence by default; any future desktop automation requires a separate capability and authorization decision.

## Consequences

- Agents can resolve “this,” “these,” and “here” across Gallery, image/video editors, CRM, diagrams, slide tools, and domains not yet invented without DOM access or platform releases.
- First-party Views and Apps share agent-facing mechanics while retaining different trust and implementation policies.
- App authors must provide schemas, useful descriptions, revision semantics, resolver/action definitions, and bounded payload behavior; Oqto validates these but does not understand every domain.
- The runtime needs a durable Context Store/catalog, Runner Broker surface, Pi adapter, Bridge capability, lifecycle events, schema-subset validator, conformance traces, and disclosure UI before App context can be advertised.
- Context may improve conversational reference resolution without becoming hidden prompt injection or ambient model cost.
- Explicit Context Attachments make user intent visible and durable while compact handles and out-of-context CLI querying prevent large dynamic App state from bloating the model window.
- The CLI becomes a stable headless and code-mode surface; native Pi and optional MCP integrations remain adapters over one authority instead of separate context systems.
- Integration with `ctx` enriches desktop handoff without creating duplicate Bundle formats or confusing live selection with durable captured evidence.

## Considered options

- **Hardcode all useful contexts in Oqto:** rejected because the platform cannot enumerate future App domains and would require releases for every new concept.
- **Let Apps send arbitrary prompts or tools to the Agent:** rejected because generated UI would gain an injection and authority-escalation channel.
- **Expose DOM, frontend stores, or browser events:** rejected as renderer-specific, non-serializable, inaccessible to headless/native hosts, and unsafe.
- **Use workspace JSON files for transient selection:** rejected as the default because it turns requester-local interaction into misleading durable domain data and lacks lifecycle/revision semantics. Files remain appropriate when the state is genuinely durable and agent-editable.
- **Merge Oqto context into `ctx`:** rejected because exact live workspace authority and Best-Effort cross-desktop capture have different ownership, privacy, precision, and persistence contracts.
- **Push every update into Chat:** rejected because selection, hover, playback, and viewport changes would wake/flood models and let Apps influence conversations without explicit user intent.
- **Inject complete selected values, provider catalogs, or schemas into every prompt:** rejected because large and dynamic App domains would consume the model context before relevance is known. Compact attachment metadata plus scoped query handles preserve user intent without paying that cost.
- **Make fine-grained Pi tools the only Agent interface:** rejected because multi-step discovery and large intermediate tool results bloat the transcript. Stable native tools may exist, but the CLI/code-mode path must filter and aggregate outside the model context.
- **Make MCP the canonical context contract or require it inside Pi:** rejected because MCP does not provide Oqto authorization, revision, revocation, or context reduction and adds another process/transport lifecycle. MCP remains an optional thin adapter for concrete external consumers.
- **Allow arbitrary code-mode mutation:** rejected because hidden mutations weaken review, stale-context protection, and auditability. Query code is read-only; actions use the explicit semantic Gate.

## Verification

1. A fixture App declares a novel topic and action using only package data; publication validates and digests its catalog/schemas without Oqto code changes.
2. Security tests reject unknown schema versions/keywords, undeclared topics/actions, oversized/deep values, spoofed platform namespaces, executable values, stale revisions, foreign Instances, and grants for a different Definition digest.
3. Gallery/slide fixtures prove selected versus visible versus focused state remain distinct and “these images” resolves through opaque authorized resource references.
4. A stale action invocation performs no mutation and returns the current revision; a matching invocation is audit-attributed to Account, Work Session, Agent, App Instance, Definition, topic revision, and action.
5. Watch conformance proves contiguous resume and explicit gap recovery across disconnect/reconnect; no client reconciles by payload text or visible order.
6. Revocation tests prove the Gate rejects immediately, connected Bridges suspend, pending/future calls fail, and reconnect cannot restore access from browser state.
7. Sensitive context requires its declared user activation; publishing context never wakes the LLM or injects a Chat message.
8. App Copy/promotion tests prove no context values, selections, subscriptions, grants, or presentation state transfer.
9. `desktop.ctx` degrades explicitly when `ctx` is absent; desktop observations preserve Best-Effort provenance and cannot invoke Oqto/App actions.
10. Pi, OqtoUI, a headless client, and a future native conformance harness consume identical versioned catalog/snapshot/action traces.
11. Composer tests prove a host-owned attachment visibly captures a proposed revision, send pins exactly that revision, changed/unavailable revisions require an explicit user choice, and reload/reconnect reconstruct the structured oqto-log part without flattening it to text.
12. Prompt-projection tests prove a high-volume attachment contributes only bounded metadata and a scoped handle; CLI query tests filter and aggregate large values outside the model transcript and return only bounded finite JSON.
13. Handle security tests reject foreign Accounts, Work Sessions, turns, providers, topics, revisions, expired handles, and unauthorized resolution modes; oqto-log contains no reusable bearer credential.
14. Code-mode conformance proves read-only queries cannot invoke actions, access ambient filesystem/network/process authority, or select acting identity; mutation remains an explicit expected-revision action with audit attribution.
15. An optional MCP adapter, if added, passes the same conformance traces and cannot retain independent context truth or grant broader authority than Pi/CLI clients.
