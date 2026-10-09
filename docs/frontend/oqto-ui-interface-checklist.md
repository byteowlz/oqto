# OqtoUI interface checklist

A curated, Oqto-adapted subset of [Vercel's Web Interface Guidelines](https://vercel.com/design/guidelines) plus Oqto-specific rules. It feeds the zero-baseline OqtoUI guardrails required by [ADR-0037](../adr/0037-oqto-ui-portable-rearrangeable-views.md) before functional `frontend/src/oqto-ui/` code lands.

How to use it:

- **[gate]** items are review-blocking for any OqtoUI feature PR that touches the relevant surface.
- **[lint]** items are candidates for mechanical enforcement (checker script, Biome rule, or test); implement enforcement when the surface first appears rather than accumulating baselines.
- Items that conflict with Vercel's originals were adapted deliberately; Vercel-brand rules (Title Case, "&" over "and", Geist) are excluded on purpose.

## Interaction

- [gate] Keyboard operates every flow; follow WAI-ARIA Authoring Patterns. Focus is trapped/moved/returned per pattern.
- [lint] Every focusable element shows a visible ring via `:focus-visible` (never remove focus styles; never `:focus`-only).
- [gate] Visual targets under 24px get hit targets ≥ 24px; mobile minimum is 44px. Labels and their controls share one generous target; no dead zones inside a control.
- [lint] `touch-action: manipulation` on interactive controls; tap highlight styled deliberately.
- [gate] Never disable paste, browser zoom, or text input ("number-only" fields accept any keystrokes and validate instead).
- [gate] URL owns shareable state: filters, tabs, selected Session/work directory, expanded panels, pagination. Test: if it is `useState` and a user would share or reload it, it belongs in the URL. Back/Forward restores scroll.
- [gate] Optimistic updates reconcile by ID against the durable authority (oqto-log); on failure, roll back and surface an error or Undo. Never reconcile by text, index, or visible order.
- [gate] Destructive actions require confirmation or an Undo window.
- [gate] Async status changes (toasts, inline validation, agent blocked-input) announce via polite `aria-live`.
- [gate] Navigation uses real links (`<a>`/`<Link>`) so Cmd/Ctrl+Click and middle-click work; never `<div>`/`<button>` for navigation.
- [gate] Keyboard shortcuts are layout-aware and display platform-specific symbols; EN/DE parity applies to shortcut help.

## Loading & perceived performance

- [gate] Spinners/skeletons have a show-delay (~150–300 ms) and a minimum visible time (~300–500 ms); no flicker on fast responses.
- [gate] Skeletons mirror final content dimensions exactly; no layout shift on resolve.
- [gate] Buttons in flight keep their label and show an inline indicator; submit disables only after submission starts and requests carry an idempotency key.
- [gate] ADR-0032 budgets stay authoritative: 50 ms tab response, 100 ms cached Session switch, 500 ms uncached content, no 50 ms input blocking, no full-shell rerender during streaming. Mutating requests target < 500 ms.
- [lint] Large lists virtualize (Sessions catalog, Files tree, timeline); images declare dimensions (no CLS); fonts preloaded and subset.
- [gate] Expensive work (parsing, diffing, highlighting) stays off the main thread or is time-sliced.

## Animation

- [gate] Motion only follows direct user action (existing Oqto rule; stricter than Vercel's "deliberate delight"). No ambient or autoplaying animation.
- [gate] Honor `prefers-reduced-motion` with a reduced variant.
- [lint] Animate compositor properties only (`transform`, `opacity`); never `transition: all`; explicit property lists.
- [gate] Animations are interruptible by input; transform-origin anchors where motion "physically" starts.

## Content & text

- [lint] `font-variant-numeric: tabular-nums` (or mono) wherever numbers compare or update in place: stats tiles, token counts, context gauges, timers, table columns.
- [gate] Every screen designs empty, sparse, dense, error, and unresolved-owner states. No dead ends: always a next step or recovery path.
- [gate] Status never relies on color alone; pair dots/colors with text labels. Charts use color-blind-safe palettes.
- [gate] Icon-only buttons carry descriptive accessible names; decoration is `aria-hidden`; semantics (native `button`, `a`, `label`, `table`) before ARIA.
- [lint] Ellipsis character `…` (not `...`) for menu items that open follow-ups ("Rename…") and in-progress states ("Saving…"). Placeholders are example values and end with `…`.
- [lint] `translate="no"` wraps brand names, code tokens, model IDs, paths, and technical identifiers so browser auto-translate cannot corrupt them (matters with EN/DE parity).
- [lint] Non-breaking spaces glue units, shortcuts, and names: `10 MB`, `⌘ K`, `Oqto UI`.
- [gate] Locale-aware dates, numbers, and delimiters from language settings (`Accept-Language`/`navigator.languages`), never IP/geolocation.
- [gate] Layouts survive short, average, and very long user/agent content; headings set `scroll-margin-top` when link-addressable.
- [gate] `<title>` reflects current context (Session name, work directory).

## Forms & composer

- [gate] Enter submits single-control forms; in `textarea`, Enter inserts a newline and ⌘/⌃+Enter submits (matches composer conventions).
- [gate] Every control has an associated label; clicking the label focuses the control.
- [gate] Don't pre-disable submit; let submission surface validation. Errors render next to their fields; on submit, focus the first error.
- [lint] Correct `type`/`inputmode`/`autocomplete`/`name` per field; spellcheck off for emails, codes, tokens, paths; trim trailing whitespace from input-method expansions before validating.
- [gate] Inputs are ≥ 16px on mobile (iOS Safari zoom) and never lose focus or value across hydration.
- [gate] Warn before navigation that would drop unsaved changes (drafts follow their ADR-0037 owner).
- [lint] Non-auth fields must not trigger password managers; OTP fields use `autocomplete="one-time-code"`.

## Mobile & PWA

- [gate] Respect safe-area insets (notches, home indicator) in fixed chrome.
- [lint] `overscroll-behavior: contain` in modals, drawers, and gesture surfaces (the Big Picture carousel bug class).
- [lint] `<html>` sets `color-scheme` matching the active Base24 scheme mode; `<meta name="theme-color">` matches the scheme background so browser/OS chrome blends.
- [gate] Verify mobile, laptop, and ultra-wide (zoom to 50% to simulate); scrollbars only where content overflows, tested with always-visible scrollbars.

## Design-system boundaries (Oqto-specific adaptations)

- Contrast improvements go through semantic role mapping only; canonical Base24 palette values stay immutable. Evaluate contrast with APCA where it is stricter than WCAG 2 (aligned with our contrast-fix workflow, not a license to edit palettes).
- Interactive states (`:hover`, `:active`, `:focus`) have more contrast than rest state, achieved via role tokens.
- Nested radii follow the ADR-0015 radius dial: child radius ≤ parent and concentric. At the default radius 0 this is trivially satisfied; it binds when users raise the dial.
- Optical alignment (±1px) is allowed and encouraged; document it inline when it looks like a mistake.

## Excluded from Vercel's list, deliberately

- Title Case headings, "&" over "and", first-person prohibitions, Geist fonts: Vercel brand, not ours.
- "Deliberate delight" autoplay motion (northern lights): conflicts with the no-ambient-animation rule.
- WCAG-vs-APCA as a wholesale switch: we adopt APCA as an evaluation tool inside the Base24 constraint, not as a palette-editing mandate.
