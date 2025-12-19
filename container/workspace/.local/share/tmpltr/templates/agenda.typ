// =============================================================================
// Meeting/Workshop Agenda Template (tmpltr edition)
// 
// Usage:
//   tmpltr compile content.toml -o out.pdf
//   tmpltr compile content.toml --brand mycompany -o out.pdf
//
// Generic template for agendas with:
// - Configurable logo and branding
// - Timetable with presenters
// - Participant list with cancellation support
// 
// Editable field IDs:
//   - workshop.title        : Event title
//   - workshop.subtitle     : Section subtitle (e.g., "Agenda")
//   - workshop.datetime     : Date and time text
//   - workshop.location     : Location/room
//   - workshop.version-date : Document version date
//   - participants.title    : Participants page title
//
// Data structure expected:
//   {
//     "brand": {
//       "logo": "path/to/logo.png",
//       "logo-width": "4cm",
//       "fonts": { "body": "Arial" },
//       "colors": { "primary": "#000000", "accent": "#179C7D", "text": "#000000" }
//     },
//     "workshop": {
//       "title": "...",
//       "subtitle": "...",
//       "datetime": "...",
//       "location": "...",
//       "version-date": "..."
//     },
//     "agenda": [ { "time": "...", "title": "...", "presenter": "...", "items": [...] }, ... ],
//     "participants": [ { "first": "...", "last": "...", "cancelled": false }, ... ],
//     "labels": {
//       "default-title": "Meeting Title",
//       "default-subtitle": "Agenda",
//       "participants-title": "Participants"
//     },
//     "lang": "en"
//   }
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, editable, editable-block, get, brand-color, brand-logo, brand-logo-image, brand-font

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// Render a single agenda item with time, title, presenter, and optional sub-items
#let render-agenda-item(item, colors) = {
  let time-cell = if item.at("time", default: none) != none {
    text(item.time)
  } else { [] }
  
  let presenter-cell = if item.at("presenter", default: none) != none and item.at("items", default: ()).len() == 0 {
    item.presenter
  } else { [] }
  
  // Main row with time, title, and presenter (for items without sub-items)
  grid(
    columns: (60pt, 1fr, 100pt),
    column-gutter: 12pt,
    align: (left, left, left),
    time-cell,
    if item.at("title", default: none) != none {
      [*#(item.title)*]
    },
    presenter-cell,
  )
  
  // Sub-items rendered separately to align presenter column correctly
  if item.at("items", default: ()).len() > 0 {
    for sub in item.items {
      v(2pt)
      grid(
        columns: (60pt, 1fr, 100pt),
        column-gutter: 12pt,
        align: (left, left, left),
        [],  // Empty time column
        [- #sub.at("text", default: "")],
        text(sub.at("presenter", default: "")),
      )
    }
  }
  
  v(12pt)
}

// Render a participant row (supports strikethrough for cancelled)
#let render-participant(p) = {
  let first = p.at("first", default: "")
  let last = p.at("last", default: "")
  let cancelled = p.at("cancelled", default: false)
  
  if cancelled {
    grid(
      columns: (100pt, 1fr),
      column-gutter: 16pt,
      strike[#first],
      strike[#last],
    )
  } else {
    grid(
      columns: (100pt, 1fr),
      column-gutter: 16pt,
      first,
      last,
    )
  }
  v(4pt)
}

// Render logo (returns nothing if no logo provided)
#let render-logo(data, logo-width) = {
  let logo-path = brand-logo(data, variant: "primary", default: none)
  
  if logo-path != none {
    image(logo-path, width: logo-width)
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
  )
  
  // ---------------------------------------------------------------------------
  // Typography
  // ---------------------------------------------------------------------------
  set text(
    font: primary-font,
    size: 10pt,
    fill: rgb(colors.text),
    lang: data.at("lang", default: "en"),
  )
  
  set par(
    justify: false,
    leading: 0.65em,
  )
  
  // List styling with small square bullet
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
  // Header section with editable fields
  // ---------------------------------------------------------------------------
  v(1cm)
  
  // Event title (stable ID: workshop.title)
  text(size: 16pt, weight: "bold")[
    #editable(
      "workshop.title", 
      get(data, "workshop.title", default: labels.at("default-title", default: "Meeting Title")),
      type: "text"
    )
  ]
  
  v(0.8cm)
  
  // Subtitle (stable ID: workshop.subtitle)
  text(size: 14pt, weight: "bold")[
    #editable(
      "workshop.subtitle", 
      get(data, "workshop.subtitle", default: labels.at("default-subtitle", default: "Agenda")),
      type: "text"
    )
  ]
  linebreak()
  
  // Metadata row: datetime/location on left, version on right
  grid(
    columns: (1fr, auto),
    {
      // Date/time (stable ID: workshop.datetime)
      editable(
        "workshop.datetime", 
        get(data, "workshop.datetime", default: labels.at("default-datetime", default: "Date | Time")),
        type: "text"
      )
      linebreak()
      // Location (stable ID: workshop.location)
      editable(
        "workshop.location", 
        get(data, "workshop.location", default: labels.at("default-location", default: "Location: TBD")),
        type: "text"
      )
    },
    align(right)[
      // Version date (stable ID: workshop.version-date)
      #editable(
        "workshop.version-date", 
        get(data, "workshop.version-date", default: labels.at("default-version", default: "Version: TBD")),
        type: "text"
      )
    ]
  )
  
  v(0.3cm)
  
  // Separator line
  line(length: 100%, stroke: 0.5pt + rgb(colors.text))
  
  v(0.8cm)
  
  // ---------------------------------------------------------------------------
  // Agenda items
  // ---------------------------------------------------------------------------
  let agenda-items = data.at("agenda", default: ())
  
  for item in agenda-items {
    render-agenda-item(item, colors)
  }
  
  // ---------------------------------------------------------------------------
  // Participants page (rendered if participants data provided)
  // ---------------------------------------------------------------------------
  let participants = data.at("participants", default: ())
  
  if participants.len() > 0 {
    pagebreak()
    
    // Logo on participants page
    place(
      top + right,
      dx: 0cm,
      dy: -1.5cm,
      render-logo(data, logo-width)
    )
    
    v(1cm)
    
    // Participants section title (stable ID: participants.title)
    text(size: 16pt, weight: "bold")[
      #editable(
        "participants.title", 
        labels.at("participants-title", default: "Participants"),
        type: "text"
      )
    ]
    
    v(0.8cm)
    
    for p in participants {
      render-participant(p)
    }
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
