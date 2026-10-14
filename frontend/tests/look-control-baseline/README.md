# Oqto Web control baseline (before Look recipes)

Issue `oqto-a8hg`. The CSS/component source predates this worktree: `943f2b02` on `feat/oqto-ui-chat-renderer-parity`. The active checkout has unrelated uncommitted changes and is **not** the source of these measurements. `oqto-jxbz` must not change the values pinned here without an explicit change to the user-approved preservation contract.

This is an **isolated Chromium CSS/component fixture**, not a full authenticated Oqto app or a macOS Desktop screenshot. It imports the *actual* `frontend/src/styles/globals.css`, Workbench `shell.css`, `applyOqtoUiScheme`, and `frontend/components/ui/button.tsx`. The shared Button samples cover a complete 5-variant × 3-size matrix: default, outline, secondary, ghost and destructive in default, sm and icon sizes. The icon-size fixtures contain an SVG, matching the shared Button's real `has-[>svg]` behavior. A `.wb-icon-button` is mounted in a `.wb-sidebar__logo` to preserve its real 30 px intrinsic size. The extended capture preserved every field of all 40 original samples and all four source SHA-256 hashes. It does not claim to cover the Button's link variant, lg/icon-sm/icon-lg sizes, every Workbench control, viewport, OS, or uncommitted theme changes.

## Reproduce

From `frontend/` after `bun install --frozen-lockfile` (Python Playwright + its Chromium installed):

```bash
python3 /home/wismut/.pi/agent/skills/webapp-testing/scripts/with_server.py \
  --server 'bunx vite --host 127.0.0.1 --port 4273 --strictPort' --port 4273 \
  -- python3 tests/look-control-baseline/capture.py
```

`--update` on `capture.py` is **only** for deliberately capturing a new, reviewed baseline; never use it to make a failing recipe pass. The default command asserts the committed `oqto-control-computed.json` for dark/light × 16 controls × default/hover/keyboard-focus-visible/active/disabled = **160 computed samples**, sampling settled styles after transitions. `--update` first refuses any change to already-pinned samples, then writes the extended golden and screenshots. It records each edge's border width, box-shadow, CSS height and measured height, padding, outline style/width/offset/color, radius and representative colors. Source SHA-256 hashes, Chromium version (147.0.7727.15 for the capture), 1280×720 viewport and DPR 1 are recorded alongside the golden; the comparison gates computed values, not raster pixels. `screenshots/` are review aids, not a pixel-equality gate.

## Pinned examples (exact values in JSON)

| Control | Normal (both modes) | Keyboard focus-visible | Note |
|---|---|---|---|
| Shared default Button | 36 px high; 0 px border; 8 px top/bottom and 16 px left/right padding; no shadow | 3 px ring **in box-shadow**, outline-style none; border color changes to ring role | Current CSS is not a ring-free house style. |
| Shared outline Button | 36 px high; 1 px border; 8/16 px padding | Same 3 px focus ring; existing box-shadow also retained | Dark mode has a 2 px y / 6 px blur shadow at alpha 0.15 **with its border**; light has a transparent 8 px y shadow value. The design-system Look contract now permits this compound elevation for the ordinary Oqto Look (govnr message `4540e6e3-40d2-4947-b4e5-a9a8be1fa5c8`), without a legacy mode. |
| Shared secondary-small Button | 32 px high; 0 px border; 0/12 px padding; no normal shadow | 3 px ring in box-shadow | Existing size differs from default. |
| Shared ghost (default/sm/icon) | 36/32/36 px high; 0 px border; 8/16, 0/12, 0/0 px top/horizontal padding; transparent normal background; no normal shadow | 3 px ring in box-shadow, dark/light ring alpha 0.5; outline-style none | Both schemes have a white/10 hover background. Active keeps hover background; disabled opacity 0.5. |
| Shared destructive (default/sm/icon) | Same geometry as ghost; 0 px border or normal shadow | 3 px ring in box-shadow; **dark alpha 0.4, light alpha 0.2**; outline-style none | Normal dark background is destructive/60; light is `oklch(0.52 0.15 24)`. Hover/active values are pinned in JSON. |
| Shared icon sizes (all five variants) | 36 × 36 px; padding 0; icon SVG child | Same per-variant ring as text controls | Default/sm text sizes: 36px high with 8/16px padding; 32px high with 0/12px padding. Outline retains its border and shadows at every size. |
| Workbench icon in sidebar logo | 30 px high; 1 px transparent border; 6 px padding; no shadow | Browser-native `outline-style: auto`, 1 px; no focus ring shadow | This is a Workbench control, **not** the shared Button. |

Focus ring/outline colors differ across dark and light; read the full computed JSON before choosing Look fixture values. Capturing real values is not approval to use ring focus in new house Looks. Govnr's approved contract direction preserves ring focus and compound elevation as *expressible* so today's Oqto appearance remains unchanged, while house preferences live in lint/review. Changing Oqto's rendering is a separate user-approved change; do not silently edit the golden.
