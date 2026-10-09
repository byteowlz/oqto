---
status: accepted
issues: [oqto-d0pc, oqto-d0pc.1]
---

# Standalone recognition with explicit local and remote ownership

## Decision

Oqto owns microphone permission/capture, language and recognition-provider choices,
preparation feedback, provisional rendering and composer submission. The independent
`@byteowlz/ears-browser` package owns inference, audio conversion, result identity,
engine completion and cancellation fencing. Foxline owns conversational Voice
Sessions, not a mandatory wrapper around dictation. eaRS and synthesis services
remain independently usable. The ownership split was confirmed in the
agent-messageboard `speech-interface-ownership` topic; no shared speculative
speech-contract package is introduced.

The existing SessionScreen/ChatView composer uses `useDictation` and the same
STTService capture path for both implementations:

- `moonshine`: local CPU/WASM streaming Tiny, explicitly English or German. Model
  GET downloads are allowed; microphone PCM stays in a local worker. No eaRS,
  Foxline or TTS service configuration is required.
- `ears`: an explicitly selected, configured eaRS WebSocket endpoint through the
  existing authenticated voice proxy. Audio goes to that endpoint only in this
  mode. Unsupported language must fail rather than use an unconfirmed default.

Changing language/provider, including a cross-tab persisted-settings change,
stops the old destination. There is no automatic server fallback after a local
failure. TTS and Foxline client-owned capability negotiation remain outside scope.
Both the legacy ChatView and `/oqto-ui` ChatPane reuse the same capture hook and
compact presentation. OqtoUI reaches browser capture and disposable draft storage
through its platform adapter, not legacy chat transport. Its platform currently
has no remote voice configuration capability, so remote recognition is explicitly
unavailable there; local recognition needs no backend speech configuration.

## Finality and chat authority

Preview text is replacement-only and separate from the editable draft. It must
never become committed text on a client silence timer, Enter, or Cancel. Only a
normalized completed result reaches the existing composer insertion callback.
IDs are opaque strings scoped to a preparation; Moonshine's native 64-bit decimal
IDs are never converted to JavaScript numbers. Remote speech boundaries commit
flushed segments; the final cumulative session word list excludes segments already
inserted. Pause is not commitment. Cancellation fences late preparation and result
callbacks, and cancels any pending auto-send.

Finish drains actual engine finality and inserts it without auto-send. Cancel drops
provisional speech while preserving text already in the composer. Existing user
opt-in auto-send only follows completed results. Submission still uses existing
chat handlers; this adds no history writer, Runner path or message reconciliation.

## Packaging and deployment

Oqto checks in a small SDK source snapshot under `frontend/vendor/ears-browser`,
consumed as a Bun `file:` dependency. `bun run sync:ears-browser-sdk` is an explicit
maintainer action that copies from an eaRS checkout. Normal builds require no
sibling repository and must not create a divergent SDK implementation.

`bun run sync:speech-runtime-assets` stages an exact upstream GitHub WASM artifact,
with archive and WASM SHA-256 validation, into gitignored public assets. The npm
0.1.5 WASM core is older than the same-version GitHub artifact: browser probes
reproduced rejection of split frontend files and absent German catalog support.
The verified GitHub artifact recognizes both pinned Tiny model directories.
The runtime/model pairing is therefore pinned by bytes, not merely a package tag.
Upstream licenses/notices accompany staged runtime assets.

The published runtime requires HTTPS/localhost and cross-origin isolation:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Development can opt in with `VITE_LOCAL_SPEECH_ISOLATION=1 bun dev`. Production
operators must configure equivalent application/runtime headers, correct WASM/ESM
MIME types and a compatible CSP. Isolation is not globally enabled by default:
external frames, cross-origin resources and opener flows need deployment review.
A missing prerequisite is a visible error, not permission to upload speech. There
is no shipped single-thread fallback. Downloaded model caches may be evicted by
the browser; this is not a promise of unconditional offline operation.

## Compact composer and model preparation (`oqto-yakf`)

The normal editor remains mounted at the same dimensions throughout dictation.
An absolutely anchored, bounded status strip shows preparation, listening,
finishing or an actionable error; provisional words occupy a separate two-line
preview and never become editor content. Language/provider/auto-send settings live
in the microphone popover, not a replacement input form. Finish commits without
sending; Cancel discards only provisional speech. Auto-send starts off in both
composers and requires explicit opt-in. The audio meter samples into its own leaf
CSS transform, without rerendering the whole chat on every volume tick.

Download progress carries cumulative bytes from the SDK, never re-sums snapshots
when filenames change. A native progress bar shows percentage and loaded/total MB
only when the complete total is known and consistent; unknown totals and other
preparation phases are indeterminate. Download completion is not recognition
readiness: initialization and microphone permission remain visibly separate.
Cancel and Retry remain available. Cached model loads can skip network transfer.

The vendored SDK is synchronized from canonical eaRS `50f809e9` (including
`9dea81fb`, `trx-cwxx` and `trx-dvgd`),
not patched only in Oqto. Its default Tiny loader uses the catalog in the exact
pinned WASM artifact: real isolated Chromium execution verified both en/de
catalogs use the prior directory URLs, eight filenames and Tiny architecture.
Catalog manifests provide aggregate declared totals; explicit custom model URL
loads retain unknown totals rather than manufacture estimates. The source proof
script is included in the snapshot; SDK tests/typecheck/build passed on its source
branch (20 SDK tests). Staging excludes AppleDouble resource-fork files and cleans
only their known generated siblings; the WASM digest remains unchanged. Generated
checksum-pinned upstream `public/speech/` assets, like visual-runtime vendor assets,
are outside application source linting; this does not exclude Oqto speech source.
This is catalog validation, not a new physical-microphone inference proof.

Layout proof: `frontend/dictation-layout.html` plus
`frontend/tests/e2e/dictation-layout-e2e.py` exercise seven states in both schemes
at 1280px and 390px (28 transitions). Chat, composer and editor bounding rectangles
remain unchanged, the editor node survives, and no horizontal overflow or page
errors occur. This is a representative shared-control fixture; actual ChatPane
component tests additionally prove send/abort authority, completed-draft retention,
preview exclusion and scope cancellation. The focused application/architecture suite
passes 97 tests; changed-file Biome/Oxlint, scoped OqtoUI typecheck, zero-baseline
architecture guardrails and the production build pass. Full TypeScript still has
exactly the unchanged 180 error signatures, full Biome the unchanged 14 errors/two
warnings, and the effect gate the same four unrelated effects. The prior explicit
`oqto-d0pc.1` deferral remains; no full-gate success is claimed. Live
deploy/microphone acceptance is still separate.

## Composer lifecycle follow-up

`oqto-qyh4` binds recognition to the unsanitized workspace/session composer
identity, independently of the persistence key. A scope change stops capture,
clears queued auto-send, and fences final delivery even before passive cleanup.
Auto-send targets the originating composer's button ref, not the first matching
button elsewhere in the document. A late Finish from an old scope cannot stop a
new capture.

`oqto-bqwq` treats completed speech as committed draft content: insertion writes
through the same best-effort draft storage helper as typed edits and clears an
older pending typed snapshot so it cannot overwrite speech. Previews never write
to draft storage; Finish still does not auto-send. Reload retention depends on
browser storage availability, as it does for typed drafts.

The follow-up retains the explicitly scoped frontend gate debt in `oqto-d0pc.1`:
comparison against unchanged `e6ce13a8` reproduced identical 180 full TypeScript
error signatures, the same 14 Biome errors/two warnings, and the same four
unapproved effects outside the speech files. These are not successful full gates;
they remain deferred rather than suppressed by this fix. The existing Node
deprecation and production chunk-size warnings are likewise retained. No live
microphone/chat/deployment acceptance is implied by mocked capture regression
tests or the production build.

## Proof and limitations

- SDK: 14 lifecycle/protocol/resampling/artifact tests, strict typecheck and browser
  ESM build pass. Remote tests replay the real eaRS wire shape, including cumulative
  teardown finality and explicit language confirmation.
- Real Chromium CPU/WASM probes recognize paced public English (4.815 seconds)
  and German (11.16 seconds) fixtures with provisional revisions and unique
  completions. A repeatable smoke harness forbids WebSockets and non-GET fetches
  in the page and STT worker; audio is not uploaded.
- Oqto: 31 targeted dictation, capture-lifecycle, draft-storage and chat-rendering
  tests pass; scoped OqtoUI typecheck and production Vite build pass. Desktop
  1280px/mobile 390px component checks show no horizontal overflow; mobile text
  inputs/selects are 16px. These are component checks, not an authenticated
  end-to-end chat/microphone demonstration.
- Physical microphone/browser permission flows, Safari/mobile inference,
  mixed-language speech, storage eviction/offline retention, production header
  deployment and a live remote-server session remain unverified.
- Full frontend typecheck/useEffect gates also fail on an archived unchanged HEAD.
  This existing debt and Node/bundle-size warnings are explicitly deferred to
  `oqto-d0pc.1`; speech changes do not suppress those gates. Structural review
  retains two explained findings: a ref-dispatched volume method misclassified
  as dead, and presentation growth for explicit controls/status. Contract checking
  preserves the submission contract. Browser host adapters now supply the optional
  authenticated remote URL; leaf metering uses a getter rather than chat-wide
  volume state. No blanket quality acknowledgement was added.
