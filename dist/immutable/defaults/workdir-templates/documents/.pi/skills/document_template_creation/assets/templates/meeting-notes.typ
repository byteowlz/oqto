// =============================================================================
// Meeting Notes Template
// @description: Structured meeting notes with attendees, agenda, decisions, and action items
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color

#let data = tmpltr-data()
#let has-brand = "brand" in data and data.brand != none
#let ensure-array(val) = if type(val) == array { val } else { () }

// Colors
#let primary = if has-brand {
  rgb(brand-color(data, "primary", default: "#1a365d"))
} else {
  rgb(get(data, "style.primary_color", default: "#1a365d"))
}
#let accent = rgb(get(data, "style.accent_color", default: "#e53e3e"))
#let muted = rgb(get(data, "style.muted_color", default: "#718096"))

// Data
#let meeting = get(data, "meeting", default: (:))
#let attendees = ensure-array(get(data, "attendees", default: ()))
#let agenda = ensure-array(get(data, "agenda", default: ()))
#let decisions = ensure-array(get(data, "decisions", default: ()))
#let actions = ensure-array(get(data, "actions", default: ()))
#let notes-text = get(data, "notes", default: "")
#let notes-text = if type(notes-text) != str { "" } else { notes-text }

// Labels
#let labels = get(data, "labels", default: (:))
#let l(key, fallback) = {
  if type(labels) == dictionary { labels.at(key, default: fallback) } else { fallback }
}

// Page setup
#set page(
  paper: "a4",
  margin: (top: 2cm, bottom: 1.5cm, left: 2cm, right: 2cm),
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Inter") } else { "Inter" },
  size: 10pt,
)
#set par(leading: 0.6em)

// --- Logo ---
#if has-brand {
  let logo = get(data, "brand.logos.primary", default: none)
  if logo != none and logo != "" {
    place(top + right, image(logo, width: 3cm))
  }
}

// =============================================================================
// HEADER
// =============================================================================

#text(size: 8pt, fill: muted, weight: "bold", tracking: 0.1em)[
  #upper(l("meeting_notes", "MEETING NOTES"))
]

#v(0.3cm)

#text(size: 22pt, weight: "bold", fill: primary)[
  #get(data, "meeting.title", default: "Meeting Title")
]

#v(0.3cm)

// Meta info line
#grid(
  columns: (auto, auto, auto, auto),
  column-gutter: 1.5cm,
  [
    #text(size: 8pt, fill: muted)[#l("date", "Date")]
    #linebreak()
    #text(size: 10pt, weight: "bold")[#get(data, "meeting.date", default: "")]
  ],
  [
    #text(size: 8pt, fill: muted)[#l("time", "Time")]
    #linebreak()
    #text(size: 10pt, weight: "bold")[#get(data, "meeting.time", default: "")]
  ],
  [
    #text(size: 8pt, fill: muted)[#l("location", "Location")]
    #linebreak()
    #text(size: 10pt, weight: "bold")[#get(data, "meeting.location", default: "")]
  ],
  [
    #text(size: 8pt, fill: muted)[#l("organizer", "Organizer")]
    #linebreak()
    #text(size: 10pt, weight: "bold")[#get(data, "meeting.organizer", default: "")]
  ],
)

#v(0.3cm)
#line(length: 100%, stroke: 1.5pt + primary)
#v(0.3cm)

// =============================================================================
// ATTENDEES
// =============================================================================

#if attendees.len() > 0 [
  #text(size: 11pt, weight: "bold", fill: primary)[#l("attendees", "Attendees")]
  #v(0.15cm)

  #let att-items = attendees.map(a => {
    let name = if type(a) == dictionary { a.at("name", default: "") } else { str(a) }
    let role = if type(a) == dictionary { a.at("role", default: "") } else { "" }
    let present = if type(a) == dictionary { a.at("present", default: true) } else { true }

    if role != "" [
      #if not present [#text(fill: muted)[(absent)] ]
      *#name* #text(size: 9pt, fill: muted)[(#role)]
    ] else [
      #if not present [#text(fill: muted)[(absent)] ]
      *#name*
    ]
  })

  #att-items.join[ #sym.dot.c ]
  #v(0.4cm)
]


// =============================================================================
// AGENDA
// =============================================================================

#if agenda.len() > 0 [
  #text(size: 11pt, weight: "bold", fill: primary)[#l("agenda", "Agenda")]
  #v(0.15cm)

  #for (i, item) in agenda.enumerate() [
    #let item-title = if type(item) == dictionary { item.at("title", default: "") } else { str(item) }
    #let item-duration = if type(item) == dictionary { item.at("duration", default: "") } else { "" }
    #let item-presenter = if type(item) == dictionary { item.at("presenter", default: "") } else { "" }
    #let item-notes = if type(item) == dictionary { item.at("notes", default: "") } else { "" }

    #grid(
      columns: (auto, 1fr, auto),
      column-gutter: 8pt,
      text(size: 10pt, weight: "bold", fill: primary)[#(i + 1).],
      [
        *#item-title*
        #if item-presenter != "" [ #text(size: 9pt, fill: muted)[(#item-presenter)] ]
        #if item-notes != "" [
          #linebreak()
          #text(size: 9pt)[#item-notes]
        ]
      ],
      if item-duration != "" { text(size: 9pt, fill: muted)[#item-duration] },
    )
    #if i < agenda.len() - 1 [
      #v(0.1cm)
      #line(length: 100%, stroke: 0.3pt + muted.transparentize(60%))
      #v(0.1cm)
    ]
  ]
  #v(0.4cm)
]


// =============================================================================
// DECISIONS
// =============================================================================

#if decisions.len() > 0 [
  #rect(
    width: 100%,
    fill: primary.transparentize(95%),
    stroke: (left: 3pt + primary),
    inset: 10pt,
    radius: (right: 3pt),
  )[
    #text(size: 11pt, weight: "bold", fill: primary)[#l("decisions", "Decisions")]
    #v(0.15cm)
    #for (i, d) in decisions.enumerate() [
      #let d-text = if type(d) == dictionary { d.at("text", default: "") } else { str(d) }
      #let d-owner = if type(d) == dictionary { d.at("owner", default: "") } else { "" }
      #text(weight: "bold")[#(i + 1).] #d-text
      #if d-owner != "" [ #text(size: 9pt, fill: muted)[(#d-owner)] ]
      #linebreak()
    ]
  ]
  #v(0.4cm)
]


// =============================================================================
// ACTION ITEMS
// =============================================================================

#if actions.len() > 0 [
  #text(size: 11pt, weight: "bold", fill: accent)[#l("action_items", "Action Items")]
  #v(0.15cm)

  #table(
    columns: (auto, 1fr, auto, auto),
    stroke: 0.5pt + muted.transparentize(50%),
    inset: 8pt,
    fill: (_, row) => if row == 0 { primary.transparentize(90%) } else { none },

    // Header
    text(size: 9pt, weight: "bold")[\#],
    text(size: 9pt, weight: "bold")[#l("action", "Action")],
    text(size: 9pt, weight: "bold")[#l("owner", "Owner")],
    text(size: 9pt, weight: "bold")[#l("due", "Due")],

    // Rows
    ..actions.enumerate().map(((i, a)) => {
      let a-text = if type(a) == dictionary { a.at("text", default: "") } else { str(a) }
      let a-owner = if type(a) == dictionary { a.at("owner", default: "") } else { "" }
      let a-due = if type(a) == dictionary { a.at("due", default: "") } else { "" }
      let a-done = if type(a) == dictionary { a.at("done", default: false) } else { false }

      (
        text(size: 9pt)[#(i + 1)],
        if a-done { strike(text(fill: muted)[#a-text]) } else { [#a-text] },
        text(size: 9pt)[#a-owner],
        text(size: 9pt)[#a-due],
      )
    }).flatten()
  )
  #v(0.4cm)
]


// =============================================================================
// NOTES
// =============================================================================

#if notes-text != "" [
  #text(size: 11pt, weight: "bold", fill: primary)[#l("notes", "Notes")]
  #v(0.15cm)
  #text(size: 10pt)[#notes-text]
  #v(0.4cm)
]


// =============================================================================
// FOOTER
// =============================================================================

#v(1fr)
#line(length: 100%, stroke: 0.5pt + muted.transparentize(50%))
#v(0.15cm)
#grid(
  columns: (1fr, 1fr),
  text(size: 8pt, fill: muted)[
    #get(data, "meeting.title", default: "") | #get(data, "meeting.date", default: "")
  ],
  align(right, text(size: 8pt, fill: muted)[
    #get(data, "footer", default: "")
  ]),
)
