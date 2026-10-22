# Runner-mediated inter-session communication

## Status

**Proposed, not accepted or implemented.** Records the discussion from 2026-10-21
so publication does not lose the distinction between recommendation and runtime
truth. No messaging permissions, Herdr integration or deployments are enabled by
this document. The installed Herdr server's per-caller ACLs have not been verified.

Related decisions: ADR-0059 (capability gates), ADR-0060 (sandbox policy),
ADR-0062 (independent authority planes), ADR-0063 (native/canonical convergence)
and ADR-0064 (durable admission versus delivery receipts).

## Context

Agents can communicate through Herdr's control socket or native Pi peer-control
surfaces. Exposing a broad control socket can confer more than message delivery:
output reads, terminal input, process launch or other supervisor actions. Hiding
commands in an extension does not prevent raw protocol requests.

Unix socket ownership/mode separates OS users, not individual Sessions sharing a
UID. Environment-supplied pane or Session identifiers are routing context, not
proof of caller identity. A restricted agent asking a more privileged peer to act
can also create a confused deputy without any kernel sandbox escape.

The inspected Pi Durable source provides a per-conversation inbox with steering,
follow-up and passive-write modes plus submission placement/settlement. That is
delivery machinery; it does not, by itself, establish inter-session authorization.

## Proposed direction

1. The runner owns a small, harness-neutral communication interface. Authenticate
   the sender from a trusted launch/connection binding, not a caller-supplied
   `from` field, mutable label, environment ID or shared-UID check alone.
2. Grant recipient and operation explicitly. Messaging, output/history reading,
   steering/interruption, terminal control and process launch are separate rights.
   Parent/child, sibling, team and unrelated-session access are separate policy
   relationships; filesystem or Workspace access grants none implicitly.
3. Use revocable Session/launch-generation-bound capabilities. Authorize admission
   and delivery, enforce principal separation, payload/rate limits and lifecycle
   fences. An implementation must address capability theft between same-UID
   processes rather than assume a private filename or environment variable solves it.
4. Hide unfiltered Herdr and equivalent native peer-control sockets from confined
   agents. A scoped socket/proxy must enforce both method and target on the trusted
   side; filtering CLI commands is insufficient. Herdr may independently implement
   scoped clients, but those must preserve the same semantic permission contract.
5. Herdr remains a process/pane transport adapter. Pi Durable's native inbox may
   receive an authorized delivery; other harnesses use their own supported adapter.
   Do not make terminal keystroke injection the canonical messaging interface.
6. Preserve attributed peer input as untrusted agent content, never human approval,
   a permission grant or system instruction. Passive delivery must not silently
   become steering or inference. Destination actions still require its own gates.
7. Use stable message IDs, payload binding, explicit acknowledgments and bounded
   deduplication. Distinguish accepted, durably admitted, placed and replied states;
   ambiguous transport failure is not permission to replay an effect.
8. For runner Sessions, `oqto-log` remains canonical history authority. Native
   delivery crosses the serialized worker/ingest interface; do not write Pi JSONL
   or introduce a second transcript authority. Native submission and canonical
   receipts must converge by explicit IDs, not text or visible order. The exact
   admission/outbox recovery mechanism requires implementation review and tests.
9. Support headless CLI/native TUI and Workbench through the same contract. A local
   runner should not require the Web frontend or central backend just to message
   another authorized local Session. Cross-runner routing adds explicit authenticated
   authority; it does not grant blanket access to remote Sessions.

## Required proof before enablement

- Raw protocol attempts cannot exceed granted method/recipient scope; unrestricted
  socket paths and inherited descriptors cannot bypass the broker.
- Spoofed sender IDs, stale launch generations, foreign principals and revoked
  grants are rejected without delivery or unauthorized disclosure.
- Parent/child rights do not expand into sibling/team/global access accidentally.
- Lost acknowledgments, reconnect, restart, duplicate IDs with changed payload,
  admission/delivery races and unavailable targets have defined safe outcomes.
- Peer input remains attributed and cannot authorize shell/provider/tool actions;
  passive delivery performs zero model calls unless separately authorized.
- Native/canonical identity mapping, durable authority and serialized writes are
  demonstrated across reload/reconnect without duplicate transcript entries.
- The same semantic permission tests cover Pi Durable and Herdr/other adapters.

## Non-goals and current boundary

This proposal is not a claim that the broker exists, that Herdr is disabled, or
that all current peer communication is scoped. Existing Herdr functionality was
not changed. Trusted parent-policy binding for isolated Herdr child launches is
still tracked separately in `oqto-v8n2.1`; durable inbox support is not a substitute.
