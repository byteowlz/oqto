# Corner-mode interaction gallery (oqto-m5sp)

Fixture-only design probe for the four-corner interaction paradigm. No
production code; open `index.html` directly in a browser.

```
firefox artifacts/oqto-ui/corner-mode-concepts/index.html
```

## The model

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
- **bottom-left — who/state:** agent state, task progress, context gauge.
- **bottom-right — do:** composer/actions; hold → quick switch
  (MRU session cards, release to open).

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
- **Hold carousels:** press-and-hold a corner, drag to highlight, release
  to equip (workspaces top-left, tools top-right, session quick-switch
  bottom-right).
- **Fuzzy search** in every list surface; matched characters are
  highlighted.
- **Procedural workspace icons:** deterministic flat geometry from the
  workspace name hash, Base24 accents only; real logos take precedence.
- **Strip vs sheet** per corner (variant A vs B).
- **Desktop composability:** variant D pins edges independently and
  toggles to the classic split.

## Status

Design probes for test-driving; nothing here is production direction.
Feedback lands in oqto-m5sp.
