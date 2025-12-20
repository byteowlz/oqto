// =============================================================================
// Meeting Agenda Template (tmpltr edition)
//
// Usage: tmpltr compile content.toml -o output.pdf
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, editable, get, brand-color, brand-logo, brand-font, md

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

#let format-attendees(attendees) = {
  attendees.map(attendee => [
    *#attendee.at("name", default: "")* - #attendee.at("role", default: "")
  ]).join(linebreak())
}

#let format-objectives(objectives) = {
  objectives.map(obj => [
    - #obj.at("description", default: "")
  ]).join(linebreak())
}

// =============================================================================
// TEMPLATE FUNCTION
// =============================================================================

#let meeting-agenda(data, doc) = {
  // Brand configuration with safe defaults
  let colors = (
    primary: brand-color(data, "primary", default: "#1e40af"),
    accent: brand-color(data, "accent", default: "#3b82f6"),
    text: brand-color(data, "text", default: "#1f2937"),
    muted: brand-color(data, "muted", default: "#6b7280"),
  )
  
  let primary-font = brand-font(data, usage: "body", default: "Inter")
  
  // Page setup
  set page(paper: "a4", margin: 2cm)
  set text(font: primary-font, size: 11pt, fill: rgb(colors.text))
  
  // Document styling
  set par(justify: true)
  set list(
    marker: text(size: 6pt, baseline: -2pt)[#sym.square.filled],
    indent: 0pt,
    body-indent: 8pt,
  )
  
  // Header
  align(center)[
    text(size: 20pt, weight: "bold", fill: rgb(colors.primary))[
      get(data, "meeting.title", default: "Meeting Agenda")
    ]
    
    v(0.5em)
    
    grid(
      columns: (1fr, 1fr),
      [*Date:* get(data, "meeting.date", default: "[Date]")], 
      [*Time:* get(data, "meeting.time", default: "[Time]")],
      [*Location:* get(data, "meeting.location", default: "[Location]")], 
      [*Duration:* get(data, "meeting.duration", default: "[Duration]")]
    )
    
    v(1em)
    
    align(left)[
      *Organizer:* get(data, "meeting.organizer.name", default: "[Your Name]")
    ]
  ]
  
  v(1.5em)
  
  // Attendees
  if data.at("meeting", default: (:)).at("attendees", default: ()).len() > 0 {
    align(left)[
      text(size: 14pt, weight: "bold", fill: rgb(colors.primary))[Attendees]
      v(0.5em)
      format-attendees(data.meeting.at("attendees", default: ()))
    ]
    v(1.5em)
  }
  
  // Meeting Objectives
  if data.at("meeting", default: (:)).at("objectives", default: ()).len() > 0 {
    align(left)[
      text(size: 14pt, weight: "bold", fill: rgb(colors.primary))[Meeting Objectives]
      v(0.5em)
      format-objectives(data.meeting.at("objectives", default: ()))
    ]
    v(1.5em)
  }
  
  // Agenda Items
  align(left)[
    text(size: 14pt, weight: "bold", fill: rgb(colors.primary))[Agenda Items]
    
    v(1em)
    
    // Opening
    align(left)[
      text(size: 12pt, weight: "600")[1. get(data, "blocks.opening.title", default: "Opening & Welcome") (get(data, "blocks.opening.duration", default: "5 min"))]
      v(0.3em)
      md(get(data, "blocks.opening.content", default: ""))
    ]
    
    v(1em)
    
    // Previous Actions
    align(left)[
      text(size: 12pt, weight: "600")[2. get(data, "blocks.previous_actions.title", default: "Previous Action Items Review") (get(data, "blocks.previous_actions.duration", default: "10 min"))]
      v(0.3em)
      md(get(data, "blocks.previous_actions.content", default: ""))
    ]
    
    v(1em)
    
    // Main Topics
    align(left)[
      text(size: 12pt, weight: "600")[3. get(data, "blocks.main_topics.title", default: "Main Discussion Topics") (get(data, "blocks.main_topics.duration", default: "[Time allocation]"))]
      v(0.3em)
      md(get(data, "blocks.main_topics.content", default: ""))
    ]
    
    v(1em)
    
    // Decisions
    align(left)[
      text(size: 12pt, weight: "600")[4. get(data, "blocks.decisions.title", default: "Decision Points") (get(data, "blocks.decisions.duration", default: "15 min"))]
      v(0.3em)
      md(get(data, "blocks.decisions.content", default: ""))
    ]
    
    v(1em)
    
    // Next Steps
    align(left)[
      text(size: 12pt, weight: "600")[5. get(data, "blocks.next_steps.title", default: "Next Steps & Action Items") (get(data, "blocks.next_steps.duration", default: "10 min"))]
      v(0.3em)
      md(get(data, "blocks.next_steps.content", default: ""))
    ]
    
    v(1em)
    
    // Wrap-up
    align(left)[
      text(size: 12pt, weight: "600")[6. get(data, "blocks.wrap_up.title", default: "Wrap-up & Next Meeting") (get(data, "blocks.wrap_up.duration", default: "5 min"))]
      v(0.3em)
      md(get(data, "blocks.wrap_up.content", default: ""))
    ]
  ]
  
  v(1.5em)
  
  // Additional Sections (if they exist and have content)
  if get(data, "blocks.preparation.content", default: "") != "" {
    align(left)[
      text(size: 14pt, weight: "bold", fill: rgb(colors.primary))[get(data, "blocks.preparation.title", default: "Pre-meeting Preparation")]
      v(0.5em)
      md(get(data, "blocks.preparation.content", default: ""))
    ]
    v(1.5em)
  }
  
  if get(data, "blocks.materials.content", default: "") != "" {
    align(left)[
      text(size: 14pt, weight: "bold", fill: rgb(colors.primary))[get(data, "blocks.materials.title", default: "Required Materials")]
      v(0.5em)
      md(get(data, "blocks.materials.content", default: ""))
    ]
    v(1.5em)
  }
  
  if get(data, "blocks.notes.content", default: "") != "" {
    align(left)[
      text(size: 14pt, weight: "bold", fill: rgb(colors.primary))[get(data, "blocks.notes.title", default: "Notes")]
      v(0.5em)
      md(get(data, "blocks.notes.content", default: ""))
    ]
  }
  
  doc
}

// =============================================================================
// APPLY TEMPLATE (always at the end)
// =============================================================================

let data = tmpltr-data()
show: doc => meeting-agenda(data, doc)