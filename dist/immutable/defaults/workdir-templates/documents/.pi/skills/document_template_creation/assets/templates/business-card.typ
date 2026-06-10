// =============================================================================
// Business Card Template
// @description: Professional business card, 85x55mm, multiple cards per A4 sheet with cut marks
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color

#let data = tmpltr-data()
#let has-brand = "brand" in data and data.brand != none
#let ensure-array(val) = if type(val) == array { val } else { () }

// Colors
#let primary = if has-brand {
  rgb(brand-color(data, "primary", default: "#1a202c"))
} else {
  rgb(get(data, "style.primary_color", default: "#1a202c"))
}
#let accent = rgb(get(data, "style.accent_color", default: "#3182ce"))
#let muted = rgb("#a0aec0")

// Card dimensions (standard ISO 7810 ID-1)
#let card-w = 85mm
#let card-h = 55mm

// Data
#let cards = ensure-array(get(data, "cards", default: ()))

// If no cards array, build a single card from top-level person/contact
#let cards = if cards.len() == 0 {
  let p = get(data, "person", default: (:))
  let c = get(data, "contact", default: (:))
  if type(p) == dictionary or type(c) == dictionary {
    ((person: p, contact: c),)
  } else { () }
} else { cards }

// --- Render a single card ---
#let render-card(card-data) = {
  let p = if type(card-data) == dictionary { card-data.at("person", default: (:)) } else { (:) }
  let p = if type(p) == dictionary { p } else { (:) }
  let c = if type(card-data) == dictionary { card-data.at("contact", default: (:)) } else { (:) }
  let c = if type(c) == dictionary { c } else { (:) }

  let name = p.at("name", default: "Your Name")
  let title = p.at("title", default: "")
  let company = p.at("company", default: "")
  let email = c.at("email", default: "")
  let phone = c.at("phone", default: "")
  let website = c.at("website", default: "")
  let address = c.at("address", default: "")

  box(width: card-w, height: card-h, clip: true, {
    // Background
    place(rect(width: 100%, height: 100%, fill: white, stroke: 0.3pt + luma(200)))

    // Accent stripe on left
    place(rect(width: 3mm, height: 100%, fill: accent, stroke: none))

    // Content
    pad(left: 6mm, right: 4mm, top: 5mm, bottom: 4mm, {
      // Logo area (top right if brand)
      if has-brand {
        let logo = get(data, "brand.logos.primary", default: none)
        if logo != none and logo != "" {
          place(top + right, image(logo, height: 10mm))
        }
      }

      // Name
      text(size: 12pt, weight: "bold", fill: primary)[#name]
      linebreak()

      // Title + company
      if title != "" {
        text(size: 8pt, fill: accent)[#title]
        linebreak()
      }
      if company != "" {
        text(size: 8pt, fill: muted)[#company]
        linebreak()
      }

      v(1fr)

      // Contact details at bottom
      set text(size: 7pt, fill: primary)

      if email != "" {
        text(fill: muted)[email ] + [ #email]
        linebreak()
      }
      if phone != "" {
        text(fill: muted)[phone ] + [ #phone]
        linebreak()
      }
      if website != "" {
        text(fill: muted)[web ] + [ #website]
        linebreak()
      }
      if address != "" {
        text(fill: muted)[addr ] + [ #address]
      }
    })
  })
}


// =============================================================================
// PAGE SETUP -- A4 sheet with multiple cards + cut marks
// =============================================================================

#set page(
  paper: "a4",
  margin: (top: 1.5cm, bottom: 1.5cm, left: 1.5cm, right: 1.5cm),
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Inter") } else { "Inter" },
  size: 9pt,
)

// Title
#align(center)[
  #text(size: 8pt, fill: muted)[Business Cards -- cut along marks]
]
#v(0.5cm)

// Calculate grid: 2 columns x 4 rows = 8 cards per sheet
#let cols = 2
#let rows = 4
#let gap = 5mm
#let mark-len = 3mm

// Fill card slots (repeat single card or use array)
#let card-slots = range(cols * rows).map(i => {
  if i < cards.len() { cards.at(i) } else if cards.len() > 0 { cards.at(0) } else { (:) }
})

// Render grid with cut marks
#align(center)[
  #for row in range(rows) [
    #for col in range(cols) [
      #let idx = row * cols + col
      #render-card(card-slots.at(idx))
      #if col < cols - 1 [ #h(gap) ]
    ]
    #if row < rows - 1 [ #v(gap) ]
  ]
]
