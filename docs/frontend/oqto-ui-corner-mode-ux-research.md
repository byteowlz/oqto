# Corner-mode UX research: what the literature says

Evidence review for the four-corner interaction paradigm (`oqto-m5sp`),
mapping established HCI findings onto the corner-mode gallery decisions.
Sources are cited inline and listed at the end.

## 1. Corners and edges are pointer-optimal — not touch-optimal

Fitts's law: acquisition time grows with distance and shrinks with target
size; the final (slow) movement phase disappears when a target is
effectively infinite [1]. Screen edges and corners are infinite targets —
for **mice**. That is why macOS menus live at the top edge and Windows
parked Start in the corner [1].

For **touch**, this advantage inverts: Avrahami's study found touchscreen
targets at screen edges actually take *longer* to hit, because a finger
can overshoot beyond the edge with no pointer feedback to correct it [1].
Combined with Hoober's grip research — 49% of users hold phones
one-handed, making the bottom-center the prime thumb zone and the far
corners the worst [2] — the implication for us is direct:

- **Desktop/big-picture:** corner buttons are the theoretically optimal
  targets. Variant D's design is evidence-aligned.
- **Mobile:** buttons belong at the **ends of the top/bottom bars**, not
  the literal screen corners. Our current placement (and the composer
  flanked by quick-switch/send at the very bottom, in the thumb zone) is
  the correct pattern — the composer-in-thumb-zone placement is itself an
  ergonomic win.
- Rounded-corner/safe-area avoidance is not just cosmetic; it is the same
  reach argument.

## 2. Radial menus: fast, distance-free, muscle-memory-friendly

The classic controlled comparison (33 subjects, CHI'88) found pie menus
~15% faster than linear menus with marginally fewer errors [3]. The
mechanism matters: items sit at **equal radial distance** with activation
regions 2–3× larger than linear rows, so selection depends on *direction,
not distance* — seek time is constant across items instead of growing
with list position [3]. Practiced users select by direction without
looking (proprioception) — which is exactly the "MGS item menu" feel.

Caveats from the same literature:

- **Item count:** works best at 2–8 items; hierarchies are needed beyond
  ~12 [3][5]. Our bottom-right radial has 4 actions (attach / voice /
  model / fork) — squarely in the sweet spot.
- **Real estate:** radial menus consume screen area and can obscure the
  focal content [3]. Anchoring ours to a corner with a backdrop and
  releasing on selection keeps this transient.
- **Novice split:** first-time users are split on preference; some find
  radial "sensitive" [3]. Weapon-wheel practice in games converged on:
  hold-to-open, icons around the ring, **details of the highlighted item
  in the center/label area**, and a mandatory **dead zone** so small
  drags don't mis-select [4]. Our MGS big-label pattern and center-cancel
  match the converged game practice; we should add an explicit dead zone
  (small drag from origin = no selection, not nearest-slice).

## 3. Hold-to-open: the marking-menu lineage supports it — with rules

Kurtenbach & Buxton's marking menus are the strongest evidence base for
our hold vocabulary. Findings: ~1/3 second dwell before the menu pops
(we use 320 ms); a drag direction selects; releasing at the center cancels
or backs up [5]. Two deeper lessons:

- **Self-rehearsing expertise:** the physical movement of selecting from
  the radial menu is *identical* to the expert "mark" gesture, so novices
  unknowingly rehearse the expert shortcut every time — the novice→expert
  transition is seamless, no second protocol to learn [5]. This is the
  strongest psychological argument for radial-hold over "toolbar with
  extra steps": it is the only pattern whose everyday use *trains* the
  power-user path.
- **Speed:** expert marks were measured ~3.5× faster than menu selection
  (0.2 s vs 0.7 s), and users perform thousands of selections — savings
  compound [5].

Risks are real and documented: long-press gestures suffer accidental
activation (users report unintentionally triggering long-press actions on
phones; movement-tolerance and adjustable delays exist as
accessibility mitigations) [6], and dual-meaning controls (tap vs hold)
are mode errors waiting for a bad day. Mitigations consistent with the
evidence:

- Movement beyond a small threshold cancels the hold (standard
  long-press recognizer behavior) [6].
- Hold must always be an **alternative path**, never the only one: tap
  targets exist for everything the radial offers (model switch also in
  the models sheet, attach also in the composer, etc.).
- A first-run affordance hint, and an accessibility setting for hold
  delay/disable.
- Predictable timing: keep 320 ms constant everywhere; inconsistent
  dwell thresholds destroy the muscle memory the pattern is supposed to
  build.

## 4. Icons: recognition beats recall, labels beat icons

Decades of testing: unlabeled icons confuse users; only a handful of
icons (magnifier, home, printer) are near-universally understood [7].
Labels also *increase target size* — an icon+label is a bigger, faster
Fitts target than an icon alone [1]. Applied to us:

- Corner buttons and radial items must keep their **big-label** reveal
  (the MGS label / weapon-wheel center detail) — and the eventual
  production UI should consider icon+microtext on the corner buttons
  themselves.
- The procedural workspace icons are identity/branding anchors
  (recognition of *place*), not action icons — that is a legitimate
  distinct use; pair them with the workspace name wherever space allows
  (the session sheet does; the corner face itself should get a label on
  first encounters or long dwell).

## 5. Spatial memory: why corners work at all

Users remember *where* things are, not *what they look like*: stable
object positions support relocation, and reflowing layouts disrupt it
badly; scaling (not reflowing) preserves spatial memory [8]. Corner
anchors are the strongest positional cues a rectangle offers. Our design
already encodes this — left = where (sessions/workspaces), right = what
(files/tools), bottom = act — and keeps the same mapping across mobile
corner mode, desktop split, and big-picture mode. The evidence says: keep
that mapping sacred, never reflow it, and let expansions scale in place
rather than reorganize.

## 6. Game-HUD psychology: unique touch without ergonomic loss

Game UX (Hodent's cognitive-science framing) treats the HUD as a system
for managing mental load and sustaining flow: minimal persistent chrome,
information on demand, recognition over recall, and never splitting
attention between the work and its controls [9]. Diegetic interfaces
(Dead Space) show the ceiling of "unique without unergonomic" — novel is
fine when the mapping is learned once and then *disappears* into
performance. That is precisely our bet: the four-corner model is a HUD
whose gestures become proprioceptive (directions, not menus), leaving the
center pure content. The zero-chrome center and exclusive-on-mobile
expansions are the cognitive-load-friendly reading; composable pinning on
desktop is the expert reading.

## Verdicts for the gallery → production path

| Decision | Evidence | Status |
| --- | --- | --- |
| Bar-end buttons (not literal corners) on mobile; true corners on desktop | [1] edges infinite for pointer, slower for touch; [2] thumb zone | already designed this way — keep |
| Composer in bottom center between the two buttons | [2] prime thumb zone | already designed this way — keep |
| Radial hold menu with ≤8 items (currently 4) | [3] sweet spot; [5] 2/4/6/8 work well | keep; do not grow past 8 |
| 320 ms dwell | [5] ~1/3 s standard | keep; never vary per corner |
| Big label for highlighted item | [4] weapon-wheel converged practice; [7] icon labels | keep |
| Center dead-zone cancel + movement cancels hold | [4] dead zones; [6] accidental activation | add dead zone to probe |
| Every hold action also reachable by plain tap | [6] dual-mode risk | models sheet exists; attach/voice/fork need tap paths |
| Stable left=where / right=what / bottom=act mapping across all modes | [8] spatial memory | keep sacred; scale, never reflow |
| Exclusive expansions on mobile, pinnable on desktop | [9] cognitive load vs expert use | keep |
| Hover previews (scroll rail) need a touch path (press-slide) | touch has no hover | add to probe |

Open questions research cannot answer for us: hold-duration fatigue in
sustained use, radial direction bias for left- vs right-handed grips,
and how quickly the marking gestures actually consolidate for daily Oqto
users — these need usability testing on the probe (measure selection
time and error rate by session count, mirroring [3]'s method).

## Sources

1. NN/g, *Fitts's Law and Its Applications in UX* (2024) — incl. Avrahami
   touchscreen edge finding. https://www.nngroup.com/articles/fitts-law/
2. S. Hoober, *How Do Users Really Hold Mobile Devices?* (UXmatters
   2013); Smashing Magazine, *The Thumb Zone* (2016). 
   https://www.smashingmagazine.com/2016/09/the-thumb-zone-designing-for-mobile-users/
3. Callahan, Hopkins, Weiser, Shneiderman, *An Empirical Comparison of
   Pie vs. Linear Menus*, CHI '88. 
   https://donhopkins.medium.com/an-empirical-comparison-of-pie-vs-linear-menus-466c6fdbba4b
4. *The history of radial menus in video games* (2026); 300mind,
   *Radial Menus in Game Design* (dead zones, wheel best practice). 
   https://medium.com/design-bootcamp/the-history-of-radial-menus-in-video-games-e6968bb1bac6 
   https://300mind.studio/blog/radial-menus-in-game-design/
5. Kurtenbach & Buxton, *User Learning and Performance with Marking
   Menus* (CHI '94); *The Limits of Expert Performance Using Hierarchic
   Marking Menus* (InterCHI '93). 
   https://www.billbuxton.com/MMUserLearn.html 
   https://www.billbuxton.com/MMExpert.html
6. Long-press accidental activation & movement-cancel behavior: UX
   StackExchange threads; React Native Gesture Handler long-press
   recognizer spec. 
   https://docs.swmansion.com/react-native-gesture-handler/docs/legacy-gestures/long-press-gesture/
7. NN/g, *Icon Usability*. https://www.nngroup.com/articles/icon-usability/
8. NN/g, *Spatial Memory: Why It Matters for UX Design* (2020). 
   https://www.nngroup.com/articles/spatial-memory/
9. C. Hodent, *The Gamer's Brain*; game-UX cognitive-load overviews. 
   https://nastyrodent.com/player-psychology-in-game-ux/
