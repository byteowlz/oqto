# Existing machine history: read-only preview

This first slice exposes existing machine-local `oqto-log` projections without waiting for native TUI attachment or execution admission. It does not import missing Pi JSONL, resume chats, start Pi, or manage provider credentials.

## Authorization and identity checklist

- Durable read authority: selected machine's `oqto-log`; event source is its saved canonical projection, not browser state or hstry.
- Backend target requires `history_read = true` and exactly one owning Account. Inventory, SSH and provider-login grants do not imply this permission.
- Runner requires `[runner.history_read]` with `account_id` and an absolute `home`, and dedicated single-user mode. The browser cannot choose a home or Account.
- Public IDs come from stored `platform_id`. Empty or ambiguous identities are omitted, and message reads reject harness IDs and ambiguous public IDs. No SessionTarget is invented, no execution admission is granted, and personal Hub routing is unchanged.
- Runner HOME, transport keys and agent sandbox policy remain unchanged. This selected history home is not a wholesale adoption of the personal HOME as the runner environment.
- Read-only means no Pi writes or execution. Existing canonical stores can undergo their own normal migrations/index maintenance; back them up consistently before enabling a new reader build.

## Surface

`POST /api/runner-targets/{target}/history` accepts `{"command":"list"}` or `{"command":"messages","session_id":"<public-id>","before":null}`. `before` accepts the opaque `next_before` cursor. Pages contain at most 50 projected messages; catalogs are capped at 5,000 entries. Responses are authenticated and `Cache-Control: no-store`; the runner request is `history_read` with the backend-supplied owning Account.

The original sidebar provides **Machines → Mac → Chats**. This is explicitly a text preview, not full rich-renderer/live-session parity. Tool data is collapsed and display-limited; file links, HTML, attachments and execution actions are not activated. Headless clients can use the same authenticated HTTP contract.

## Reload and disconnect

Queries are deployment/Account/machine/Session/cursor-scoped and immediately garbage-collected on unmount. There is no persistent history cache, prompt queue or fallback machine. Reopening refetches the projection. Offline errors are explicit; already loaded content may remain visible until the view closes. Account changes/logout unmount the private view and dispose its queries. No content-based reconciliation is performed.

## Verification

Runner tests cover native-store reads, public-vs-harness IDs, unchanged Pi sentinel, Account denial before source access, shared-mode denial, path injection, mutation rejection and redacted responses. Backend target tests cover explicit grants and single-owner restrictions. Frontend tests cover durable IDs, cursor forwarding, escaped text, absence of an input surface, offline no-fallback and query disposal.

Mac proof: 20 existing canonical stores contained 575 chats and approximately 89,000 messages. All 20 stores were backed up before activation. The live mTLS reader listed 575 chats, projected a 28-message page, and denied a different Account; message contents were not printed. Personal Pi session files, credentials and settings were not modified by the reader. This does not claim all 3,035 discovered Pi JSONL files are represented or that live TUI control works.

Hub/browser proof: authenticated HTTP listed 575 chats and opened a 28-message page; anonymous access was 401, an unapproved target 403, and malformed input 400 without reflecting its value. The original UI rendered 575 selectable chats and 28 unique durable message IDs, fit the viewport, displayed the read-only label, and contained no composer or file links in the dialog. The old proof-profile provider-login grant is disabled.

Gates: 101 runner tests; 295 backend tests passed and one ignored; three native Mac reader tests; 11 focused frontend tests; focused component/OqtoUI typechecks; Clippy with warnings denied; frontend and crate-boundary guards; generated types regenerated/formatted with no semantic diff. The existing Rust AI guardrail gate still reports the same 41 baseline findings (30 are external-test-module classification); no new history-module findings or baseline relaxation. That existing debt is explicitly retained for this narrowly scoped preview, not claimed as a passing gate.
