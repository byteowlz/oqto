# Oqto Web control baseline (before Look recipes)

Issue `oqto-a8hg`. The CSS/component source predates this worktree: `943f2b02` on `feat/oqto-ui-chat-renderer-parity`. The active checkout has unrelated uncommitted changes and is **not** the source of these measurements. `oqto-jxbz` must not change the values pinned here without an explicit change to the user-approved preservation contract.

This is an **isolated Chromium CSS/component fixture**, not a full authenticated Oqto app or a macOS Desktop screenshot. It imports the *actual* `frontend/src/styles/globals.css`, Workbench `shell.css`, `applyOqtoUiScheme`, and `frontend/components/ui/button.tsx`. The shared Button samples cover default, outline and secondary-small; a `.wb-icon-button` is mounted in a `.wb-sidebar__logo` to preserve its real 30 px intrinsic size. It does not claim to cover every Workbench control, variant, viewport, OS, or uncommitted theme changes.

## Reproduce

From `frontend/` after `bun install --frozen-lockfile` (Python Playwright + its Chromium installed):

```bash
python3 /home/wismut/.pi/agent/skills/webapp-testing/scripts/with_server.py \
  --server 'bunx vite --host 127.0.0.1 --port 4273 --strictPort' --port 4273 \
  -- python3 tests/look-control-baseline/capture.py
```

`--update` on `capture.py` is **only** for deliberately capturing a new, reviewed baseline; never use it to make a failing recipe pass. The default command asserts the committed `oqto-control-computed.json` for dark/light × four controls × default/hover/keyboard-focus-visible/active/disabled, sampling settled styles after transitions. It records each edge's border width, box-shadow, CSS height and measured height, padding, outline style/width/offset/color, radius and representative colors. Source SHA-256 hashes, Chromium version (147.0.7727.15 for the capture), 1280×720 viewport and DPR 1 are recorded alongside the golden; the comparison gates computed values, not raster pixels. `screenshots/` are review aids, not a pixel-equality gate.

## Pinned examples (exact values in JSON)

| Control | Normal (both modes) | Keyboard focus-visible | Note |
|---|---|---|---|
| Shared default Button | 36 px high; 0 px border; 8 px top/bottom and 16 px left/right padding; no shadow | 3 px ring **in box-shadow**, outline-style none; border color changes to ring role | Current CSS is not a ring-free house style. |
| Shared outline Button | 36 px high; 1 px border; 8/16 px padding | Same 3 px focus ring; existing box-shadow also retained | Dark mode has a 2 px y / 6 px blur shadow at alpha 0.15 **with its border**; light has a transparent 8 px y shadow value. This conflicts with a strict "never border plus shadow" recipe unless the user intentionally changes appearance or the recipe represents legacy values. |
| Shared secondary-small Button | 32 px high; 0 px border; 0/12 px padding; no normal shadow | 3 px ring in box-shadow | Existing size differs from default. |
| Workbench icon in sidebar logo | 30 px high; 1 px transparent border; 6 px padding; no shadow | Browser-native `outline-style: auto`, 1 px; no focus ring shadow | This is a Workbench control, **not** the shared Button. |

Focus ring/outline colors differ across dark and light; read the full computed JSON before choosing Look fixture values. Capturing real values is not approval to keep ring focus in new user-authored Looks, and substituting the Studio's proposed ring-free Oqto values would change the current Web appearance. Resolve that contradiction across Web/Desktop/Design Studio **after** this baseline, not by silently editing the golden.
