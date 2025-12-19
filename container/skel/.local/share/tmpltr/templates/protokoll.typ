// =============================================================================
// Meeting Protocol/Minutes Template (tmpltr edition)
// 
// Usage:
//   tmpltr compile content.toml -o out.pdf
//   tmpltr compile content.toml --brand mycompany -o out.pdf
//
// Generic template for meeting protocols/minutes with:
// - Configurable logo and branding
// - Meeting metadata (date, location, attendees)
// - Agenda items with decisions and action items
// - Action item summary
// 
// Editable field IDs:
//   - meeting.title        : Meeting title
//   - meeting.subtitle     : Document subtitle (e.g., "Protokoll")
//   - meeting.date         : Meeting date
//   - meeting.time         : Meeting time (start - end)
//   - meeting.location     : Location/room
//   - meeting.author       : Protocol author
//
// Data structure expected:
//   {
//     "brand": { ... },
//     "meeting": {
//       "title": "...",
//       "subtitle": "Protokoll",
//       "date": "2025-01-15",
//       "time": "10:00 - 12:00",
//       "location": "...",
//       "author": "..."
//     },
//     "attendees": [ { "name": "...", "role": "...", "absent": false }, ... ],
//     "items": [
//       {
//         "title": "...",
//         "discussion": "...",
//         "decisions": ["..."],
//         "actions": [{ "task": "...", "responsible": "...", "due": "..." }]
//       }
//     ],
//     "labels": { ... },
//     "lang": "de"
//   }
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, editable, editable-block, get, brand-color, brand-logo, brand-font

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// Render logo (returns nothing if no logo provided)
#let render-logo(data, logo-width) = {
  let logo-path = brand-logo(data, variant: "primary", default: none)
  
  if logo-path != none {
    image(logo-path, width: logo-width)
  }
}

// Render an attendee row
#let render-attendee(a, labels) = {
  let name = a.at("name", default: "")
  let role = a.at("role", default: "")
  let absent = a.at("absent", default: false)
  
  let absent-marker = if absent {
    [ _(#labels.at("absent", default: "absent"))_]
  } else {
    []
  }
  
  grid(
    columns: (1fr, 1fr),
    column-gutter: 16pt,
    if absent { text(fill: luma(150))[#name#absent-marker] } else { name },
    if absent { text(fill: luma(150))[#role] } else { role },
  )
  v(2pt)
}

// Render an agenda item with discussion, decisions, and actions
#let render-item(item, index, colors, labels) = {
  // Item title
  text(weight: "bold")[#(index + 1). #item.at("title", default: "")]
  v(0.3em)
  
  // Discussion notes
  if item.at("discussion", default: "") != "" {
    text(item.discussion)
    v(0.5em)
  }
  
  // Decisions
  let decisions = item.at("decisions", default: ())
  if decisions.len() > 0 {
    text(weight: "bold", fill: rgb(colors.accent))[#labels.at("decisions", default: "Decisions"):]
    v(0.2em)
    for decision in decisions {
      [- #decision]
    }
    v(0.5em)
  }
  
  // Action items
  let actions = item.at("actions", default: ())
  if actions.len() > 0 {
    text(weight: "bold", fill: rgb(colors.accent))[#labels.at("actions", default: "Action Items"):]
    v(0.2em)
    for action in actions {
      let task = action.at("task", default: "")
      let responsible = action.at("responsible", default: "")
      let due = action.at("due", default: "")
      [- #task]
      if responsible != "" or due != "" {
        text(size: 9pt, fill: luma(100))[ (#responsible#if due != "" [, #labels.at("due", default: "due"): #due])]
      }
      linebreak()
    }
    v(0.5em)
  }
  
  v(0.8em)
}

// Render all action items as a summary table
#let render-action-summary(items, colors, labels) = {
  let all-actions = ()
  
  for item in items {
    let actions = item.at("actions", default: ())
    for action in actions {
      all-actions.push((
        topic: item.at("title", default: ""),
        task: action.at("task", default: ""),
        responsible: action.at("responsible", default: ""),
        due: action.at("due", default: ""),
      ))
    }
  }
  
  if all-actions.len() > 0 {
    table(
      columns: (auto, 1fr, auto, auto),
      stroke: 0.5pt,
      fill: (col, row) => if row == 0 { rgb(colors.accent).lighten(80%) } else { none },
      [*#labels.at("topic", default: "Topic")*],
      [*#labels.at("task", default: "Task")*],
      [*#labels.at("responsible", default: "Responsible")*],
      [*#labels.at("due-date", default: "Due")*],
      ..all-actions.map(a => (
        a.topic,
        a.task,
        a.responsible,
        a.due,
      )).flatten()
    )
  }
}

// =============================================================================
// TEMPLATE FUNCTION
// =============================================================================

#let tmpltr_template(data, doc) = {
  // ---------------------------------------------------------------------------
  // Brand configuration with safe defaults
  // ---------------------------------------------------------------------------
  let colors = (
    primary: brand-color(data, "primary", default: "#000000"),
    accent: brand-color(data, "accent", default: "#179C7D"),
    text: brand-color(data, "text", default: "#000000"),
  )
  
  let logo-width-str = get(data, "brand.logo-width", default: "4cm")
  let logo-width = if type(logo-width-str) == str {
    eval(logo-width-str)
  } else {
    4cm
  }
  
  let primary-font = brand-font(data, usage: "body", default: "Arial")
  
  // ---------------------------------------------------------------------------
  // Labels for localization (configurable defaults)
  // ---------------------------------------------------------------------------
  let labels = data.at("labels", default: (:))
  
  // ---------------------------------------------------------------------------
  // Page setup
  // ---------------------------------------------------------------------------
  set page(
    paper: "a4",
    margin: (
      top: 2.5cm,
      bottom: 2cm,
      left: 2.5cm,
      right: 2.5cm,
    ),
    numbering: "1 / 1",
    number-align: center + bottom,
  )
  
  // ---------------------------------------------------------------------------
  // Typography
  // ---------------------------------------------------------------------------
  set text(
    font: primary-font,
    size: 10pt,
    fill: rgb(colors.text),
    lang: data.at("lang", default: "de"),
  )
  
  set par(
    justify: true,
    leading: 0.65em,
  )
  
  // List styling
  set list(
    marker: text(size: 6pt, baseline: 0pt)[#sym.square.filled],
    indent: 0pt,
    body-indent: 8pt,
  )
  
  // ---------------------------------------------------------------------------
  // Logo placement (top right) - only if provided
  // ---------------------------------------------------------------------------
  place(
    top + right,
    dx: 0cm,
    dy: -1.5cm,
    render-logo(data, logo-width)
  )
  
  // ---------------------------------------------------------------------------
  // Header section
  // ---------------------------------------------------------------------------
  v(1cm)
  
  let meeting = data.at("meeting", default: (:))
  
  // Meeting title
  text(size: 16pt, weight: "bold")[
    #editable(
      "meeting.title", 
      meeting.at("title", default: labels.at("default-title", default: "Meeting Title")),
      type: "text"
    )
  ]
  
  v(0.5cm)
  
  // Subtitle (e.g., "Protokoll")
  text(size: 14pt, weight: "bold")[
    #editable(
      "meeting.subtitle", 
      meeting.at("subtitle", default: labels.at("default-subtitle", default: "Protokoll")),
      type: "text"
    )
  ]
  
  v(0.5cm)
  
  // Separator line
  line(length: 100%, stroke: 0.5pt + rgb(colors.text))
  
  v(0.5cm)
  
  // ---------------------------------------------------------------------------
  // Meeting metadata
  // ---------------------------------------------------------------------------
  grid(
    columns: (auto, 1fr),
    column-gutter: 1em,
    row-gutter: 0.4em,
    
    text(weight: "bold")[#labels.at("date", default: "Datum"):],
    editable("meeting.date", meeting.at("date", default: ""), type: "text"),
    
    text(weight: "bold")[#labels.at("time", default: "Zeit"):],
    editable("meeting.time", meeting.at("time", default: ""), type: "text"),
    
    text(weight: "bold")[#labels.at("location", default: "Ort"):],
    editable("meeting.location", meeting.at("location", default: ""), type: "text"),
    
    text(weight: "bold")[#labels.at("author", default: "Protokoll"):],
    editable("meeting.author", meeting.at("author", default: ""), type: "text"),
  )
  
  v(0.8cm)
  
  // ---------------------------------------------------------------------------
  // Attendees
  // ---------------------------------------------------------------------------
  let attendees = data.at("attendees", default: ())
  
  if attendees.len() > 0 {
    text(size: 12pt, weight: "bold")[#labels.at("attendees", default: "Teilnehmer")]
    v(0.4cm)
    
    for a in attendees {
      render-attendee(a, labels)
    }
    
    v(0.8cm)
  }
  
  // Separator line
  line(length: 100%, stroke: 0.5pt + rgb(colors.text))
  
  v(0.8cm)
  
  // ---------------------------------------------------------------------------
  // Agenda items with discussions, decisions, actions
  // ---------------------------------------------------------------------------
  let items = data.at("items", default: ())
  
  if items.len() > 0 {
    text(size: 12pt, weight: "bold")[#labels.at("agenda", default: "Tagesordnung")]
    v(0.6cm)
    
    for (index, item) in items.enumerate() {
      render-item(item, index, colors, labels)
    }
  }
  
  // ---------------------------------------------------------------------------
  // Action item summary (if any actions exist)
  // ---------------------------------------------------------------------------
  let has-actions = items.any(item => item.at("actions", default: ()).len() > 0)
  
  if has-actions {
    v(0.5cm)
    line(length: 100%, stroke: 0.5pt + rgb(colors.text))
    v(0.8cm)
    
    text(size: 12pt, weight: "bold")[#labels.at("action-summary", default: "Offene Punkte")]
    v(0.6cm)
    
    render-action-summary(items, colors, labels)
  }
  
  // ---------------------------------------------------------------------------
  // Additional document content
  // ---------------------------------------------------------------------------
  doc
}

// =============================================================================
// APPLY TEMPLATE
// =============================================================================

#let data = tmpltr-data()
#show: doc => tmpltr_template(data, doc)
