# OqtoUI Big Picture concept study

Six standalone interactive mockups for a Workspace/work-directory overview above ordinary OqtoUI layouts. These are design probes, not production architecture or a selected direction.

## Open

From the repository root:

```bash
python3 -m http.server 4173 --directory artifacts/oqto-ui/big-picture-concepts
```

Then open <http://localhost:4173/>.

## Concepts

1. **Spatial Matrix** — Niri-like vertical work-directory rows with a horizontal overview tile and Session episodes. Tests spatial memory and two-axis navigation.
2. **Project Cinema** — selected project as a large branded hero, with streaming-style Session shelves. Tests whether strong identity and familiar media browsing help on large displays.
3. **Focus + Filmstrip** — persistent compact project navigator and one selected Session filmstrip. Tests focused desktop/touch navigation.
4. **Activity Atlas** — space and ordering reflect urgency and active work. Tests supervision and triage against the loss of stable spatial positions.
5. **Anchor Carousel** — projects move vertically through a fixed focus band while Sessions move horizontally inside the selected project. Tests whether a constant focus position improves orientation with wheel, swipe, keyboard, and gamepad-like input.
6. **Stage + Rail** — a stats hero for the selected repository above an analog rail of identical-height project rows; the focused row carries Session tiles. Tests whether uniform row heights make continuous scrolling feel natural while a stable hero holds the detail.

## Interaction

- `1`–`6`: switch concepts
- Arrow keys: move through work directories and Sessions
- `Enter`: inspect the selected overview/Session
- `/`: search everything
- Pointer/touch: activate tiles and controls
- **Design notes**: compare intent, strength, and research question

## Asset discovery represented

The fixture uses optimized transparent-white derivatives of real repository assets discovered under existing `./logo/` directories for Oqto, mmry, eavs, foxline, h8, tmz, and lst. The mockup deliberately tests both wide banner and square icon usage against dark, unboxed surfaces. Production discovery remains a separate validated `.oqto/ui.toml` contract; merely finding a `./logo/` directory must not become identity or authorization.

## What to evaluate

- Can you remember where a work directory and Session lives after switching away?
- Do the overview tiles provide enough value compared with opening the latest Session?
- Which model reveals blocked/waiting agent work without making the screen anxious?
- Do real logos help recognition, and should Oqto infer candidates from `./logo/` when explicit `.oqto` metadata is absent?
- Is horizontal Session history useful at realistic counts, or does it need grouping/search thresholds?
- Which model survives TV distance, gamepad navigation, desktop density, and narrow touch screens?

## Non-goals

- No backend or filesystem discovery is connected.
- “Enter workspace” buttons intentionally stop at the preview; the destination OqtoUI layout is not mocked here.
- Session order, row ranking, preview summaries, and status are scripted research data.
- No ADR direction should be selected from screenshots alone; exercise keyboard/touch navigation and compare multiple screen sizes.
