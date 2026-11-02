# ADR-0076: Session media refs retain their original authority

- Status: Accepted; Runner implementation qualified, mobile rollout pending
- Date: 2026-10-31
- Issue: oqto-j08e
- Related: [placement-neutral capabilities](0039-capability-endpoints-are-placement-neutral.md), [native Session graph convergence](0063-native-session-graph-convergence.md)

## Context

Personal history can inline megabytes of screenshot base64 that a small client
cannot display. Raising every transport limit increases memory/transfer cost and
does not create a usable image experience. Pi owns its native JSONL; compressing or
rewriting that durable source would conflate model history with display assets.

## Decision

Use one `oqto-media` descriptor/renderer/cache implementation across native personal
history, managed durable history and workspace raster previews. Project explicit
image blocks at the history wire boundary into portable
original-byte SHA-256 refs with MIME, dimensions and size. Remove identical image
mirrors only within a tool display payload, never reconcile or deduplicate messages
by text, order or array length. Do not write Pi JSONL or canonical history.

Retrieve original or thumbnail through an admitted Runner capability. Native
personal media retains SessionObserve, Workspace roots, public/native binding,
native source pin and descriptor/snapshot checks. Managed media uses the selected
oqto-log message content, not a live-file fallback or private raw control envelopes;
validate its exact binding and selected record before/after derivation. Workspace
previews use their admitted file capability and stable descriptors. A hash is an
identifier, never a grant or global lookup key.

The backend derives a mandatory managed Workspace ceiling (owned personal history,
within a shared root, or exact remote workdir); the Runner checks it before reading
content or using a cache. Browser/peer requests never choose a filesystem source.
Read-only target resolution checks shared membership independently of prompt/start
permission, before cached placement access and again before media delivery. The
drive resolver remains unchanged for execution/mutation. Raw Runner connections
still require their existing OS-principal authority.

Thumbnails are deterministic, versioned, bounded display products. Originals remain
authoritative; a configurable private Runner-local thumbnail cache is disposable,
never a durable media authority. Cache hits retain source and grant checks; byte
limits, idle expiry and LRU eviction bound retention. Re-derivation does
not require media migration once the native Session is legitimately admitted on a
new Runner. This does not grant Session handoff or automatic source re-pinning.
The headless peer CLI uses the same action and writes a new private local download,
returning only a metadata receipt rather than base64 into agent context. Managed
clients use a typed Runner read or authenticated binary HTTP endpoint. Workbench
renders thumbnails and links to exact originals; it does not promote locality to
authority or silently send originals to a local client.

## Consequences

- History refreshes no longer carry original image bytes; display fetch is explicit.
- Full-image retrieval alone has a larger bounded response allowance.
- Runtime decoding needs pixel/allocation/input limits and a bounded worker pool.
- A client must implement thumbnail fetch/render/cache; old placeholders do not
  constitute rollout completion.
- Revocation prevents future disclosure, not erasure of already downloaded bytes.
- Source refusal and oversized upstream TUI frames are separate protocol concerns.

## Verification and rollout

See [implementation, wire contract and proof](../development/personal-session-media.md).
`just test-personal-media` covers the real native page budget and signed HTTPS/CLI
retrieval, grant denial, source replacement, foreign refs and revocation. Physical
phone rendering/transfer acceptance remains open; no live service was activated.
