// =============================================================================
// Habit Tracker Template
// @description: Monthly habit tracking grid with habits as rows and days as columns
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get

#let data = tmpltr-data()
#let ensure-array(val) = if type(val) == array { val } else { () }

// Colors
#let primary = rgb(get(data, "style.primary_color", default: "#2d3748"))
#let accent = rgb(get(data, "style.accent_color", default: "#38a169"))
#let fail-color = rgb(get(data, "style.fail_color", default: "#e53e3e"))
#let muted = rgb("#a0aec0")
#let bg = rgb(get(data, "style.background_color", default: "#ffffff"))

// Data
#let title = get(data, "title", default: "Habit Tracker")
#let month = get(data, "month", default: "February")
#let year = get(data, "year", default: "2026")
#let num-days = get(data, "days_in_month", default: 28)
#let habits = ensure-array(get(data, "habits", default: ()))
#let quote-text = get(data, "quote", default: "")
#let notes-text = get(data, "notes", default: "")
#let notes-text = if type(notes-text) != str { "" } else { notes-text }

// Labels
#let labels = get(data, "labels", default: (:))
#let l(key, fallback) = {
  if type(labels) == dictionary { labels.at(key, default: fallback) } else { fallback }
}

// Page setup -- landscape for more day columns
#set page(
  paper: "a4",
  flipped: true,
  margin: (top: 1cm, bottom: 1cm, left: 1cm, right: 1cm),
  fill: bg,
)

#set text(
  font: "Inter",
  size: 9pt,
  fill: primary,
)

// =============================================================================
// HEADER
// =============================================================================

#grid(
  columns: (1fr, auto),
  [
    #text(size: 22pt, weight: "bold", fill: primary)[#title]
    #h(8pt)
    #text(size: 14pt, fill: accent)[#month #year]
  ],
  if quote-text != "" {
    align(right + bottom, text(size: 8pt, style: "italic", fill: muted)["#quote-text"])
  },
)

#v(0.3cm)
#line(length: 100%, stroke: 1pt + primary)
#v(0.3cm)


// =============================================================================
// HABIT GRID
// =============================================================================

// Default habits if none provided
#let effective-habits = if habits.len() == 0 {(
  (name: "Exercise", icon: "", color: ""),
  (name: "Read", icon: "", color: ""),
  (name: "Meditate", icon: "", color: ""),
  (name: "Hydration", icon: "", color: ""),
  (name: "Sleep 8h", icon: "", color: ""),
  (name: "No sugar", icon: "", color: ""),
  (name: "Journal", icon: "", color: ""),
  (name: "Walk 10k", icon: "", color: ""),
)} else { habits }

#let n-days = if type(num-days) == int { num-days } else { 28 }
#let cell-size = 14pt

// Check mark helper
#let habit-cell(done: none, day: 0, habit-idx: 0) = {
  if done == true {
    box(width: cell-size, height: cell-size,
      align(center + horizon, text(size: 10pt, fill: accent, weight: "bold")[#sym.checkmark]))
  } else if done == false {
    box(width: cell-size, height: cell-size,
      align(center + horizon, text(size: 8pt, fill: fail-color)[#sym.times]))
  } else {
    // Empty cell for filling in
    box(width: cell-size, height: cell-size,
      align(center + horizon, text(size: 6pt, fill: muted.transparentize(50%))[#sym.dot.c]))
  }
}

// Build the table
#let header-row = (
  text(size: 8pt, weight: "bold")[#l("habit", "Habit")],
  ..range(1, n-days + 1).map(d => align(center, text(size: 7pt, weight: "bold")[#d])),
  align(center, text(size: 7pt, weight: "bold")[#l("total", sym.sum)]),
)

#let data-rows = effective-habits.enumerate().map(((hi, habit)) => {
  let h-name = if type(habit) == dictionary { habit.at("name", default: "") } else { str(habit) }
  let h-icon = if type(habit) == dictionary { habit.at("icon", default: "") } else { "" }
  let h-data = if type(habit) == dictionary { habit.at("data", default: ()) } else { () }
  let h-data = if type(h-data) == array { h-data } else { () }
  let h-color = if type(habit) == dictionary { habit.at("color", default: "") } else { "" }
  let row-accent = if h-color != "" { rgb(h-color) } else { accent }

  let display-name = if h-icon != "" { h-icon + " " + h-name } else { h-name }

  // Count completions
  let completed = h-data.filter(d => d == true).len()

  (
    text(size: 8.5pt, weight: "bold")[#display-name],
    ..range(n-days).map(d => {
      let val = if d < h-data.len() { h-data.at(d) } else { none }
      align(center, habit-cell(done: val, day: d, habit-idx: hi))
    }),
    align(center, text(size: 8pt, weight: "bold", fill: if completed > 0 { accent } else { muted })[
      #if h-data.len() > 0 [#completed] else [ ]
    ]),
  )
}).flatten()

#table(
  columns: (8em, ..range(n-days).map(_ => cell-size + 4pt), auto),
  stroke: 0.3pt + muted.transparentize(40%),
  inset: 3pt,
  fill: (col, row) => {
    if row == 0 { primary.transparentize(92%) }
    else if col == 0 { primary.transparentize(96%) }
    else { none }
  },
  ..header-row,
  ..data-rows,
)


// =============================================================================
// BOTTOM: Stats + Notes
// =============================================================================

#v(0.3cm)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 1cm,

  // Legend
  [
    #text(size: 8pt, fill: muted)[
      #box(width: 10pt, height: 10pt, baseline: 2pt,
        align(center + horizon, text(size: 8pt, fill: accent)[#sym.checkmark]))
      = #l("completed", "Completed")
      #h(12pt)
      #box(width: 10pt, height: 10pt, baseline: 2pt,
        align(center + horizon, text(size: 7pt, fill: fail-color)[#sym.times]))
      = #l("missed", "Missed")
      #h(12pt)
      #box(width: 10pt, height: 10pt, baseline: 2pt,
        align(center + horizon, text(size: 5pt, fill: muted)[#sym.dot.c]))
      = #l("pending", "Pending")
    ]
  ],

  // Notes
  if notes-text != "" {
    align(right, text(size: 8pt, fill: muted)[#notes-text])
  },
)
