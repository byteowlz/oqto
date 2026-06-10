// =============================================================================
// Hand-Drawn Week Calendar Template
// @description: Weekly planner with a sketchy, hand-drawn aesthetic
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get

#let data = tmpltr-data()

// --- Configuration ---
#let ink = rgb(get(data, "style.ink_color", default: "#2c3e50"))
#let accent = rgb(get(data, "style.accent_color", default: "#c0392b"))
#let highlight-col = rgb(get(data, "style.highlight_color", default: "#f9e79f"))
#let paper-bg = rgb(get(data, "style.paper_color", default: "#fdf6e3"))
#let line-color = rgb(get(data, "style.line_color", default: "#d5c4a1"))
#let purple = rgb("#8e44ad")
#let green = rgb("#27ae60")

#let title-text = get(data, "week.title", default: "This Week")
#let subtitle-text = get(data, "week.subtitle", default: "")
#let week-number = get(data, "week.number", default: "")
#let year-text = get(data, "week.year", default: "2026")
#let quote-text = get(data, "week.quote", default: "")

// Day data
// Ensure arrays: if the value is not an array (e.g. empty string from defaults), use ()
#let ensure-array(val) = if type(val) == array { val } else { () }
#let days = ensure-array(get(data, "days", default: ()))
#let weekly-notes = get(data, "notes", default: "")
#let weekly-notes = if type(weekly-notes) != str { "" } else { weekly-notes }
#let weekly-goals = ensure-array(get(data, "goals", default: ()))
#let priorities = ensure-array(get(data, "priorities", default: ()))

// --- Sketchy drawing helpers ---

#let wobble-offset(seed) = {
  let offsets = (0.3pt, -0.5pt, 0.7pt, -0.2pt, 0.4pt, -0.6pt, 0.1pt, -0.3pt, 0.5pt, -0.4pt)
  offsets.at(calc.rem(calc.abs(seed), offsets.len()))
}

#let sketchy-hline(width: 100%, stroke-width: 0.8pt, color: ink, seed: 0) = {
  let w = wobble-offset(seed)
  let w2 = wobble-offset(seed + 3)
  box(width: width, height: stroke-width + 1.5pt, {
    place(dy: w, line(length: 100%, stroke: stroke-width + 0.1pt + color))
    place(dy: w2 + 0.3pt, line(length: 100%, stroke: (stroke-width * 0.3) + color.transparentize(70%)))
  })
}

#let sketchy-box(
  width: auto,
  height: auto,
  box-fill: none,
  stroke-color: ink,
  stroke-width: 0.8pt,
  inset: 8pt,
  seed: 0,
  body,
) = {
  let w1 = wobble-offset(seed)
  let w2 = wobble-offset(seed + 1)
  let w3 = wobble-offset(seed + 2)
  let w4 = wobble-offset(seed + 3)

  box(width: width, height: height, {
    if box-fill != none {
      place(rect(width: 100%, height: 100%, fill: box-fill, stroke: none, radius: 1pt))
    }
    place(top + left, dy: w1, line(length: 100%, stroke: stroke-width + stroke-color))
    place(bottom + left, dy: w2, line(length: 100%, stroke: stroke-width + stroke-color))
    place(top + left, dx: w3, line(start: (0pt, 0pt), end: (0pt, 100%), stroke: stroke-width + stroke-color))
    place(top + right, dx: w4, line(start: (0pt, 0pt), end: (0pt, 100%), stroke: stroke-width + stroke-color))
    // Shadow pass
    place(top + left, dy: w1 + 0.5pt, line(length: 100%, stroke: (stroke-width * 0.3) + stroke-color.transparentize(75%)))
    place(bottom + left, dy: w2 - 0.5pt, line(length: 100%, stroke: (stroke-width * 0.3) + stroke-color.transparentize(75%)))
    if type(inset) == dictionary {
      pad(top: inset.at("y", default: 8pt), bottom: inset.at("y", default: 8pt),
          left: inset.at("x", default: 8pt), right: inset.at("x", default: 8pt), body)
    } else {
      pad(inset, body)
    }
  })
}

#let checkbox(checked: false, seed: 0) = {
  let w = wobble-offset(seed)
  box(width: 11pt, height: 11pt, baseline: 2pt, {
    place(rect(width: 100%, height: 100%, stroke: 0.8pt + ink, radius: 1pt))
    place(dx: w, dy: wobble-offset(seed + 1), rect(width: 100%, height: 100%, stroke: 0.3pt + ink.transparentize(70%), radius: 1.5pt))
    if checked {
      place(dx: 1.5pt, dy: 0.5pt, line(start: (1pt, 5pt), end: (3.5pt, 8pt), stroke: 1.5pt + accent))
      place(dx: 1.5pt, dy: 0.5pt, line(start: (3.5pt, 8pt), end: (8pt, 1.5pt), stroke: 1.5pt + accent))
    }
  })
  h(3pt)
}

#let doodle-star(size: 8pt, color: accent) = {
  box(width: size, height: size, baseline: 1pt, {
    place(center + horizon, text(size: size, fill: color, sym.diamond.filled))
  })
  h(2pt)
}

#let circled-num(num, color: accent) = {
  box(baseline: 2pt, circle(
    radius: 8pt,
    stroke: 0.8pt + color,
    fill: color.transparentize(90%),
    align(center + horizon, text(size: 8pt, weight: "bold", fill: color)[#num])
  ))
  h(3pt)
}


// =============================================================================
// PAGE SETUP -- portrait A4
// =============================================================================
#set page(
  paper: "a4",
  margin: (top: 1cm, bottom: 0.8cm, left: 1.2cm, right: 1.2cm),
  fill: paper-bg,
  background: {
    // Faint ruled lines (notebook feel)
    for i in range(0, 50) {
      let y = 1cm + i * 0.58cm
      place(top + left, dy: y, dx: 1.2cm,
        line(length: 100% - 2.4cm, stroke: 0.25pt + line-color.transparentize(60%)))
    }
    // Left margin line (school notebook red)
    place(top + left, dx: 0.9cm,
      line(start: (0pt, 0.4cm), end: (0pt, 100% - 0.4cm),
        stroke: 0.5pt + rgb("#e6b0aa").transparentize(40%)))
  },
)

#set text(
  font: ("Marker Felt", "Noteworthy", "Bradley Hand", "Comic Sans MS", "Segoe Print"),
  size: 10.5pt,
  fill: ink,
)
#set par(leading: 0.5em)


// =============================================================================
// HEADER
// =============================================================================

#align(center)[
  #text(size: 28pt, weight: "bold", fill: accent, tracking: 0.04em)[#title-text]
  #if week-number != "" or subtitle-text != "" [
    #v(-0.15cm)
    #text(size: 11pt, fill: ink.transparentize(35%))[
      #if week-number != "" [Week #week-number]
      #if week-number != "" and subtitle-text != "" [ #sym.dot.c ]
      #if subtitle-text != "" [#subtitle-text]
      #if year-text != "" [ #sym.dot.c #year-text]
    ]
  ]
]

#v(-0.05cm)
#align(center)[#sketchy-hline(width: 50%, color: accent, seed: 42)]

#if quote-text != "" [
  #v(0.05cm)
  #align(center)[
    #text(size: 9pt, style: "italic", fill: ink.transparentize(35%))[
      "#quote-text"
    ]
  ]
]

#v(0.15cm)


// =============================================================================
// DAY ROWS
// =============================================================================

#let render-task(task, index, day-index) = {
  let task-text = if type(task) == dictionary { task.at("text", default: "") } else { str(task) }
  let task-done = if type(task) == dictionary { task.at("done", default: false) } else { false }
  let task-time = if type(task) == dictionary { task.at("time", default: "") } else { "" }

  [
    #checkbox(checked: task-done, seed: day-index * 10 + index)
    #if task-time != "" [#text(size: 8pt, fill: accent)[#task-time ]
    ]#if task-done [#strike(text(fill: ink.transparentize(40%))[#task-text])
    ] else [#task-text
    ]
  ]
}

#let render-day-row(day, index) = {
  let day-name = if type(day) == dictionary { day.at("name", default: "") } else { "" }
  let day-date = if type(day) == dictionary { day.at("date", default: "") } else { "" }
  let day-tasks = if type(day) == dictionary { day.at("tasks", default: ()) } else { () }
  let day-notes = if type(day) == dictionary { day.at("notes", default: "") } else { "" }
  let is-weekend = index >= 5

  sketchy-box(
    width: 100%,
    box-fill: if is-weekend { highlight-col.transparentize(70%) } else { white.transparentize(30%) },
    stroke-color: if is-weekend { accent.transparentize(30%) } else { ink.transparentize(50%) },
    stroke-width: if is-weekend { 0.9pt } else { 0.6pt },
    inset: (x: 8pt, y: 5pt),
    seed: index * 7,
  )[
    #grid(
      columns: (8em, 1fr),
      column-gutter: 8pt,
      // Left column: day name + date
      [
        #text(size: 13pt, weight: "bold", fill: if is-weekend { accent } else { ink })[#day-name]
        #if day-date != "" [
          \ #text(size: 8.5pt, fill: ink.transparentize(40%))[#day-date]
        ]
      ],
      // Right column: tasks + notes
      [
        #if day-tasks.len() > 0 [
          #for (ti, task) in day-tasks.enumerate() [
            #render-task(task, ti, index) \
          ]
        ] else [
          #v(1pt)
          #sketchy-hline(color: line-color.transparentize(30%), stroke-width: 0.25pt, seed: index * 5)
          #v(6pt)
          #sketchy-hline(color: line-color.transparentize(30%), stroke-width: 0.25pt, seed: index * 5 + 1)
        ]
        #if day-notes != "" [
          #v(1pt)
          #text(size: 9pt, style: "italic", fill: ink.transparentize(25%))[#day-notes]
        ]
      ],
    )
  ]
}

// Default empty days if none provided
#let effective-days = if days.len() == 0 {(
  (name: "Monday", date: "", tasks: (), notes: ""),
  (name: "Tuesday", date: "", tasks: (), notes: ""),
  (name: "Wednesday", date: "", tasks: (), notes: ""),
  (name: "Thursday", date: "", tasks: (), notes: ""),
  (name: "Friday", date: "", tasks: (), notes: ""),
  (name: "Saturday", date: "", tasks: (), notes: ""),
  (name: "Sunday", date: "", tasks: (), notes: ""),
)} else { days }

// Render all 7 day rows stacked vertically
#for (i, day) in effective-days.enumerate() [
  #render-day-row(day, i)
  #if i < effective-days.len() - 1 [ #v(4pt) ]
]


// =============================================================================
// BOTTOM PANEL: Priorities | Notes | Goals
// =============================================================================

#v(6pt)

#grid(
  columns: (1fr, 1fr, 1fr),
  column-gutter: 6pt,

  // --- Priorities ---
  sketchy-box(
    width: 100%,
    box-fill: rgb("#ebdef0").transparentize(60%),
    stroke-color: purple.transparentize(40%),
    stroke-width: 0.7pt,
    inset: 6pt,
    seed: 77,
  )[
    #text(size: 11pt, weight: "bold", fill: purple)[
      #doodle-star(color: purple) Priorities
    ]
    #v(3pt)
    #if priorities.len() > 0 [
      #for (i, p) in priorities.enumerate() [
        #{ let p-text = if type(p) == dictionary { p.at("text", default: "") } else { str(p) }
           let p-done = if type(p) == dictionary { p.at("done", default: false) } else { false }
           [#circled-num(i + 1, color: purple)
            #if p-done [#strike(text(fill: ink.transparentize(40%))[#p-text])
            ] else [#p-text]] } \
      ]
    ] else [
      #for i in range(3) [
        #circled-num(i + 1, color: purple) #box(width: 70%, sketchy-hline(color: line-color.transparentize(20%), stroke-width: 0.25pt, seed: 80 + i)) \
      ]
    ]
  ],

  // --- Notes ---
  sketchy-box(
    width: 100%,
    box-fill: white.transparentize(40%),
    stroke-color: ink.transparentize(50%),
    stroke-width: 0.6pt,
    inset: 6pt,
    seed: 88,
  )[
    #text(size: 11pt, weight: "bold", fill: ink)[Notes]
    #v(3pt)
    #if weekly-notes != "" [
      #text(size: 9.5pt)[#weekly-notes]
    ] else [
      #for i in range(3) [
        #sketchy-hline(color: line-color.transparentize(30%), stroke-width: 0.25pt, seed: 90 + i)
        #v(8pt)
      ]
    ]
  ],

  // --- Goals ---
  sketchy-box(
    width: 100%,
    box-fill: rgb("#d5f5e3").transparentize(60%),
    stroke-color: green.transparentize(40%),
    stroke-width: 0.7pt,
    inset: 6pt,
    seed: 99,
  )[
    #text(size: 11pt, weight: "bold", fill: green)[Goals]
    #v(3pt)
    #if weekly-goals.len() > 0 [
      #for (i, wg) in weekly-goals.enumerate() [
        #{ let wg-text = if type(wg) == dictionary { wg.at("text", default: "") } else { str(wg) }
           let wg-done = if type(wg) == dictionary { wg.at("done", default: false) } else { false }
           [#checkbox(checked: wg-done, seed: 100 + i)
            #if wg-done [#strike(text(fill: ink.transparentize(40%))[#wg-text])
            ] else [#wg-text]] } \
      ]
    ] else [
      #for i in range(3) [
        #checkbox(seed: 100 + i) #box(width: 70%, sketchy-hline(color: line-color.transparentize(20%), stroke-width: 0.25pt, seed: 110 + i)) \
      ]
    ]
  ],
)


// =============================================================================
// FOOTER
// =============================================================================
#v(1fr)
#align(center)[
  #text(size: 7.5pt, fill: ink.transparentize(55%))[
    #get(data, "footer", default: "made with tmpltr")
  ]
]
