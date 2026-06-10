// =============================================================================
// Hand-Drawn Week Calendar -- Landscape
// @description: Weekly planner in landscape A4 with 7-column grid and sketchy hand-drawn style
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

#let ensure-array(val) = if type(val) == array { val } else { () }
#let days = ensure-array(get(data, "days", default: ()))
#let weekly-notes = get(data, "notes", default: "")
#let weekly-notes = if type(weekly-notes) != str { "" } else { weekly-notes }
#let weekly-goals = ensure-array(get(data, "goals", default: ()))
#let priorities = ensure-array(get(data, "priorities", default: ()))


// =============================================================================
// SKETCHY DRAWING HELPERS
// =============================================================================

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
  box(width: 10pt, height: 10pt, baseline: 2pt, {
    place(rect(width: 100%, height: 100%, stroke: 0.7pt + ink, radius: 1pt))
    place(dx: w, dy: wobble-offset(seed + 1), rect(width: 100%, height: 100%, stroke: 0.25pt + ink.transparentize(70%), radius: 1.5pt))
    if checked {
      place(dx: 1pt, dy: 0.5pt, line(start: (1pt, 4.5pt), end: (3pt, 7pt), stroke: 1.3pt + accent))
      place(dx: 1pt, dy: 0.5pt, line(start: (3pt, 7pt), end: (7pt, 1pt), stroke: 1.3pt + accent))
    }
  })
  h(2pt)
}

#let doodle-star(size: 7pt, color: accent) = {
  box(width: size, height: size, baseline: 1pt, {
    place(center + horizon, text(size: size, fill: color, sym.diamond.filled))
  })
  h(1.5pt)
}

#let circled-num(num, color: accent) = {
  box(baseline: 2pt, circle(
    radius: 6.5pt,
    stroke: 0.7pt + color,
    fill: color.transparentize(90%),
    align(center + horizon, text(size: 7pt, weight: "bold", fill: color)[#num])
  ))
  h(2pt)
}


// =============================================================================
// PAGE SETUP -- landscape A4
// =============================================================================
#set page(
  paper: "a4",
  flipped: true,
  margin: (top: 0.8cm, bottom: 0.6cm, left: 0.8cm, right: 0.8cm),
  fill: paper-bg,
  background: {
    // Faint ruled lines (notebook feel)
    for i in range(0, 35) {
      let y = 0.8cm + i * 0.55cm
      place(top + left, dy: y, dx: 0.8cm,
        line(length: 100% - 1.6cm, stroke: 0.2pt + line-color.transparentize(65%)))
    }
    // Left margin line (school notebook red)
    place(top + left, dx: 0.55cm,
      line(start: (0pt, 0.3cm), end: (0pt, 100% - 0.3cm),
        stroke: 0.4pt + rgb("#e6b0aa").transparentize(45%)))
  },
)

#set text(
  font: ("Marker Felt", "Noteworthy", "Bradley Hand", "Comic Sans MS", "Segoe Print"),
  size: 9pt,
  fill: ink,
)
#set par(leading: 0.45em)


// =============================================================================
// HEADER
// =============================================================================

#grid(
  columns: (1fr, auto, 1fr),
  // Left: quote
  align(left + horizon)[
    #if quote-text != "" [
      #text(size: 7.5pt, style: "italic", fill: ink.transparentize(40%))[
        "#quote-text"
      ]
    ]
  ],
  // Center: title
  align(center + horizon)[
    #text(size: 24pt, weight: "bold", fill: accent, tracking: 0.04em)[#title-text]
    #if week-number != "" or subtitle-text != "" [
      #h(8pt)
      #text(size: 9pt, fill: ink.transparentize(35%))[
        #if week-number != "" [W#week-number]
        #if week-number != "" and subtitle-text != "" [ #sym.dot.c ]
        #if subtitle-text != "" [#subtitle-text]
        #if year-text != "" [ #sym.dot.c #year-text]
      ]
    ]
  ],
  // Right: empty or small decoration
  align(right + horizon)[
    #text(size: 7.5pt, fill: ink.transparentize(50%))[
      #doodle-star(color: accent)
      #doodle-star(color: accent.transparentize(40%))
      #doodle-star(color: accent.transparentize(70%))
    ]
  ],
)

#v(0.1cm)
#sketchy-hline(width: 100%, color: accent.transparentize(30%), seed: 42, stroke-width: 0.6pt)
#v(0.15cm)


// =============================================================================
// 7-COLUMN DAY GRID
// =============================================================================

#let day-col-gap = 5pt

#let render-day-column(day, index) = {
  let day-name = if type(day) == dictionary { day.at("name", default: "") } else { "" }
  let day-date = if type(day) == dictionary { day.at("date", default: "") } else { "" }
  let day-tasks = if type(day) == dictionary { day.at("tasks", default: ()) } else { () }
  let day-tasks = if type(day-tasks) == array { day-tasks } else { () }
  let day-notes = if type(day) == dictionary { day.at("notes", default: "") } else { "" }
  let is-weekend = index >= 5

  sketchy-box(
    width: 100%,
    box-fill: if is-weekend { highlight-col.transparentize(70%) } else { white.transparentize(30%) },
    stroke-color: if is-weekend { accent.transparentize(30%) } else { ink.transparentize(50%) },
    stroke-width: if is-weekend { 0.8pt } else { 0.5pt },
    inset: (x: 4pt, y: 4pt),
    seed: index * 7,
  )[
    // Day header
    #align(center)[
      #text(size: 10pt, weight: "bold", fill: if is-weekend { accent } else { ink })[
        #day-name
      ]
    ]
    #if day-date != "" [
      #align(center)[
        #text(size: 7pt, fill: ink.transparentize(40%))[#day-date]
      ]
    ]
    #v(1pt)
    #sketchy-hline(color: if is-weekend { accent.transparentize(50%) } else { ink.transparentize(60%) }, stroke-width: 0.4pt, seed: index * 3 + 10)
    #v(3pt)

    // Tasks
    #if day-tasks.len() > 0 [
      #for (ti, task) in day-tasks.enumerate() [
        #{
          let task-text = if type(task) == dictionary { task.at("text", default: "") } else { str(task) }
          let task-done = if type(task) == dictionary { task.at("done", default: false) } else { false }
          let task-time = if type(task) == dictionary { task.at("time", default: "") } else { "" }
          [
            #checkbox(checked: task-done, seed: index * 10 + ti)
            #if task-time != "" [#text(size: 6.5pt, fill: accent)[#task-time] ]
            #if task-done [#strike(text(size: 8pt, fill: ink.transparentize(40%))[#task-text])
            ] else [#text(size: 8pt)[#task-text]]
          ]
        } \
      ]
    ] else [
      // Blank lines for handwriting
      #v(2pt)
      #for i in range(4) [
        #sketchy-hline(color: line-color.transparentize(30%), stroke-width: 0.2pt, seed: index * 5 + i + 20)
        #v(10pt)
      ]
    ]

    // Day notes
    #if day-notes != "" [
      #v(2pt)
      #text(size: 7.5pt, style: "italic", fill: ink.transparentize(25%))[#day-notes]
    ]
  ]
}

// Default empty days
#let effective-days = if days.len() == 0 {(
  (name: "Monday", date: "", tasks: (), notes: ""),
  (name: "Tuesday", date: "", tasks: (), notes: ""),
  (name: "Wednesday", date: "", tasks: (), notes: ""),
  (name: "Thursday", date: "", tasks: (), notes: ""),
  (name: "Friday", date: "", tasks: (), notes: ""),
  (name: "Saturday", date: "", tasks: (), notes: ""),
  (name: "Sunday", date: "", tasks: (), notes: ""),
)} else { days }

// Render 7 columns -- stretch to fill available height so bottom panel anchors to page bottom
#block(height: 1fr)[
  #grid(
    columns: (1fr,) * calc.min(7, effective-days.len()),
    column-gutter: day-col-gap,
    ..effective-days.enumerate().map(((i, day)) => render-day-column(day, i))
  )
]

// =============================================================================
// BOTTOM PANEL: Priorities | Notes | Goals -- inline horizontal strip
// =============================================================================

#v(4pt)

// Use a single sketchy-box spanning the full width, with 3 internal columns
#sketchy-box(
  width: 100%,
  box-fill: white.transparentize(50%),
  stroke-color: ink.transparentize(50%),
  stroke-width: 0.6pt,
  inset: (x: 6pt, y: 5pt),
  seed: 77,
)[
  #grid(
    columns: (1fr, 1fr, 1fr),
    column-gutter: 12pt,

    // --- Priorities ---
    [
      #text(size: 9pt, weight: "bold", fill: purple)[
        #doodle-star(color: purple) Priorities
      ]
      #v(2pt)
      #if priorities.len() > 0 [
        #for (i, p) in priorities.enumerate() [
          #{
            let p-text = if type(p) == dictionary { p.at("text", default: "") } else { str(p) }
            let p-done = if type(p) == dictionary { p.at("done", default: false) } else { false }
            [#circled-num(i + 1, color: purple)
             #if p-done [#strike(text(size: 8pt, fill: ink.transparentize(40%))[#p-text])
             ] else [#text(size: 8pt)[#p-text]]]
          } \
        ]
      ] else [
        #for i in range(3) [
          #circled-num(i + 1, color: purple) #box(width: 65%, sketchy-hline(color: line-color.transparentize(20%), stroke-width: 0.2pt, seed: 80 + i)) \
        ]
      ]
    ],

    // --- Notes ---
    [
      #text(size: 9pt, weight: "bold", fill: ink)[Notes]
      #v(2pt)
      #if weekly-notes != "" [
        #text(size: 8pt)[#weekly-notes]
      ] else [
        #for i in range(3) [
          #sketchy-hline(color: line-color.transparentize(30%), stroke-width: 0.2pt, seed: 90 + i)
          #v(7pt)
        ]
      ]
    ],

    // --- Goals ---
    [
      #text(size: 9pt, weight: "bold", fill: green)[Goals]
      #v(2pt)
      #if weekly-goals.len() > 0 [
        #for (i, wg) in weekly-goals.enumerate() [
          #{
            let wg-text = if type(wg) == dictionary { wg.at("text", default: "") } else { str(wg) }
            let wg-done = if type(wg) == dictionary { wg.at("done", default: false) } else { false }
            [#checkbox(checked: wg-done, seed: 100 + i)
             #if wg-done [#strike(text(size: 8pt, fill: ink.transparentize(40%))[#wg-text])
             ] else [#text(size: 8pt)[#wg-text]]]
          } \
        ]
      ] else [
        #for i in range(3) [
          #checkbox(seed: 100 + i) #box(width: 65%, sketchy-hline(color: line-color.transparentize(20%), stroke-width: 0.2pt, seed: 110 + i)) \
        ]
      ]
    ],
  )
]


// =============================================================================
// FOOTER
// =============================================================================
#align(center)[
  #v(2pt)
  #text(size: 6.5pt, fill: ink.transparentize(55%))[
    #get(data, "footer", default: "made with tmpltr")
  ]
]
