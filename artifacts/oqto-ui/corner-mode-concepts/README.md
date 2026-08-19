# Corner-mode interaction gallery (oqto-m5sp)

Fixture-only design probe for the four-corner interaction paradigm. No
production code; open `index.html` directly in a browser.

```
firefox artifacts/oqto-ui/corner-mode-concepts/index.html
```

## The model (v3)

Hierarchy discovered in the backend and now explicit in the probe:
**workspace (tenant) → workdir → session.**

- **Navigator (tap top-left):** full-screen on mobile. A vertical
  **workdir ribbon** (logos + micro-names, independently scrollable)
  sits in **one vertical line with the top-left button**; the session
  list scrolls beside it. Tenant header on top with accent underline and
  a SWITCH button (tap parity for the hold carousel).
- **Tenant switching is deliberate:** hold top-left → tenant carousel
  (MGS style); workspaces are identity/authorization boundaries, not a
  third rail. Accent hue tints the tenant header.
- **Fuzzy searches the whole tenant:** empty query browses the selected
  workdir; typing searches sessions across all workdirs (rows grow a
  workdir chip).
- **Corner grammar: tap = do, hold = choose.** Tap top-right = Files
  (anchor tool); blank hold-release = previous tool. Tap bottom-left =
  previous session (instant); hold = cross-tenant MRU fan. Tap
  bottom-right = send; hold = **quarter-arc radial** (≤4 items, corner
  dead zone = cancel, release on item = commit).
- **Controller mapping (variant D, PAD overlay):** L1 navigator/tenant,
  R1 files/tool wheel, L2 previous-session/fan, R2 send/radial;
  stick-steer while holding, release-on-item commits, release-on-nothing
  cancels. Immediate actions (send) are tap-only on all surfaces.

## The original model

The corner mode is the classic 3-way split collapsed into the edges. The
top bar keeps title + `[session-id]`, the bottom bar keeps the status
strip; the four corner buttons live at the bar ends (safe from rounded
corners / safe areas on mobile). Expansions inherit their sidebar's
identity: left surfaces use the dark rail surface, right surfaces the
lighter files-pane surface, so desktop split and corner mode share one
spatial memory.

- **top-left — where:** sessions of the current workspace; hold →
  workspace switcher carousel (procedural icons for icon-less projects).
- **top-right — with what:** workspace tools (files/editor/terminal/…).
- **composer row (bottom):** the prompt input sits **between** the two
  bottom corner buttons — quick switch on the left, send on the right.
- **bottom-left — quick switch:** tap → fuzzy MRU session list; hold →
  session cards fan (release to open).
- **bottom-right — send:** taps send the prompt; hold → radial menu with
  attach / voice / **model switcher** / fork (drag to highlight, release
  to select; the model sheet updates the status bar).
- **status bar (below composer):** slim strip with agent status, model,
  context gauge, version; tap to expand agent state and task progress.

## Variants (keys 1–4)

| Key | Variant | Expansions |
| --- | ------- | ---------- |
| A | Edge strips | narrow edge surfaces, exclusive on mobile |
| B | Quadrant sheets | quarter sheets with full content + fuzzy search |
| C | MGS carousels | hold-carousels everywhere, MGS item-menu feel |
| D | Big picture (desktop) | fullscreen content, pinnable edges, morphs into the classic 3-way split |

## Things to feel out

- **Vocabulary toggle** (panel, top right): `tap=open · hold=quick` vs
  inverted `press=carousel · release=equip`.
- **Hold carousels:** press-and-hold, drag to highlight, release to equip
  (workspaces top-left, tools top-right). Bottom-right hold opens a
  **radial menu** (attach / voice / model / fork); bottom-left hold opens
  the quick-switch fan.
- **Quick-scroll rail** on the right edge middle: one square dot per
  message cluster (user dots accent-filled), hover for a truncated message
  preview, click to jump. Only appears once the history earns it (8+
  messages).
- **Fuzzy search** in every list surface; matched characters are
  highlighted.
- **Procedural workspace icons:** deterministic flat geometry from the
  workspace name hash, Base24 accents only; real logos take precedence.
- **Strip vs sheet** per corner (variant A vs B).
- **Desktop composability:** variant D pins edges independently and
  toggles to the classic split.

## Evidence

Interaction decisions are grounded in the UX research review in
`docs/frontend/oqto-ui-corner-mode-ux-research.md` (Fitts's law,
pie-menu and marking-menu studies, thumb-zone research, weapon-wheel
practice, spatial memory, game-HUD cognitive load).

## Status

Design probes for test-driving; nothing here is production direction.
Feedback lands in oqto-m5sp.
