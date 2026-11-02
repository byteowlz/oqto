# Wake phrases in Oqto (waqy)

Status: implemented for both web frontends (legacy UI and OqtoUI), off by default.
Epic `oqto-623a`. Detection is evidence only — never authentication, identity or
permission (waqy ADR 0001). A heard phrase does exactly what the microphone button
does: it starts dictation. It never sends, approves or changes a setting.

## What runs where

| Part | Where | Notes |
|---|---|---|
| Release folder (`manifest.json` + ONNX) | operator model store, outside git | served by the backend, never committed |
| Hash-verified serving | `backend/crates/oqto/src/api/handlers/wake.rs` | every request re-verifies; a mismatch refuses the release |
| Browser engine | `frontend/vendor/waqy-browser/waqy-engine.mjs` (Apache-2.0, provenance pinned) | the same engine the standalone waqy UI loads |
| `WakeService` | `frontend/lib/voice/wake-service.ts` | capture, half-duplex, detection, release verification |
| Shared setting + UI | `lib/voice/recognition-settings.ts`, `components/voice/RecognitionControls.tsx` | used by the legacy composer and OqtoUI through `useDictation` |
| Half-duplex gate | `frontend/lib/voice/playback-gate.ts`, fed by `tts-service.ts` | detection ignores the mic while the app speaks (+ engine echo tail) |

## Operator setup

```toml
[voice]
enabled = true

[voice.wake]            # optional
# path to an immutable waqy release; "~" is expanded
wake_release = "~/.local/share/foxline/waqy-models/spqx-r3"
```

With no `wake_release`, `/api/features` omits `voice.wake`, the toggle is absent,
and nothing downloads. No weights live in this repository.

## User behaviour

- Off by default. The toggle appears only when the server offers a release.
- The microphone opens only while enabled and dictation is idle; disabling closes it.
- While Oqto speaks, detection is muted (the speaker output reaches the mic), and a
  short echo tail after playback is dropped too.
- Only the selected phrase counts; a detection starts dictation; the wake audio
  itself is not part of the transcript because capture starts after it.
- Nothing about the audio or the detection is uploaded; the release is fetched once
  from the app's own origin (`/api/voice/wake/...`), hash-checked in the browser as
  well as on the server.

## Verification

- Backend: manifest/file verification, tamper refusal, path/escape/symlink refusal,
  range checks (`cargo test -p oqto --lib wake::`).
- Frontend: release verification before engine creation, tamper refusal, detection
  refractory, playback muting, missing microphone, load error, and the shared
  control's off-by-default/evidence text (`bunx vitest run tests/wake*`).
- `vite build` bundles ONNX Runtime from the pinned package and stages its WASM into
  `public/speech/ort/` (same-origin, no CDN).

## Not done yet

- Indicator and settings surface beyond the composer toggle: blocked on a Studio
  variant (`oqto-623a.3`); the ears `wake/r1` proposals are for eaRS, not Oqto.
- A real release (`spqx-r3`) was not available on this machine, so end-to-end
  detection with real audio is unproven here; the tests use synthetic bytes.
- Trigger actions other than "start dictation" (e.g. stop phrases) are eaRS
  behaviour today and are not part of this contract.
