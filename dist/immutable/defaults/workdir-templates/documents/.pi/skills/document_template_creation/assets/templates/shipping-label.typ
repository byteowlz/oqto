// =============================================================================
// Shipping Label Template
// @description: Standard shipping label with sender, recipient, and shipment details
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color

#let data = tmpltr-data()
#let has-brand = "brand" in data and data.brand != none

// Colors
#let primary = if has-brand {
  rgb(brand-color(data, "primary", default: "#1a202c"))
} else {
  rgb(get(data, "style.primary_color", default: "#1a202c"))
}
#let accent = rgb(get(data, "style.accent_color", default: "#e53e3e"))
#let muted = rgb("#718096")

// Data
#let sender = get(data, "sender", default: (:))
#let recipient = get(data, "recipient", default: (:))
#let shipment = get(data, "shipment", default: (:))

// Labels
#let labels = get(data, "labels", default: (:))
#let l(key, fallback) = {
  if type(labels) == dictionary { labels.at(key, default: fallback) } else { fallback }
}

// Helper: address block
#let address-block(addr, size: 11pt) = {
  if type(addr) != dictionary { return [] }
  let name = addr.at("name", default: "")
  let company = addr.at("company", default: "")
  let street = addr.at("street", default: "")
  let street2 = addr.at("street2", default: "")
  let postal = addr.at("postal", default: "")
  let city = addr.at("city", default: "")
  let state = addr.at("state", default: "")
  let country = addr.at("country", default: "")
  let phone = addr.at("phone", default: "")

  set text(size: size)

  if company != "" { text(weight: "bold")[#company]; linebreak() }
  if name != "" { [#name]; linebreak() }
  if street != "" { [#street]; linebreak() }
  if street2 != "" { [#street2]; linebreak() }
  {
    let city-line = ()
    if postal != "" { city-line.push(postal) }
    if city != "" { city-line.push(city) }
    if state != "" { city-line.push(state) }
    if city-line.len() > 0 { city-line.join(", "); linebreak() }
  }
  if country != "" { text(weight: "bold")[#upper(country)]; linebreak() }
  if phone != "" { text(size: size - 1pt, fill: muted)[Tel: #phone] }
}


// =============================================================================
// PAGE SETUP -- 4x6 inch label (standard shipping) on A4 with multiple labels
// =============================================================================

#let label-w = 15cm
#let label-h = 10cm

#set page(
  paper: "a4",
  margin: (top: 1.5cm, bottom: 1.5cm, left: 1.5cm, right: 1.5cm),
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Inter") } else { "Inter" },
  size: 10pt,
  fill: primary,
)

// Center label on page
#align(center)[
  #box(width: label-w, height: label-h, {

    // Outer border
    place(rect(width: 100%, height: 100%, stroke: 1.5pt + primary, radius: 3pt))

    pad(12pt, {

      // === TOP: Sender (small) ===
      text(size: 7pt, fill: muted, weight: "bold", tracking: 0.05em)[#upper(l("from", "FROM"))]
      v(0.1cm)
      box(width: 55%, {
        address-block(sender, size: 8pt)
      })

      v(0.15cm)
      line(length: 100%, stroke: 0.5pt + muted.transparentize(50%))
      v(0.15cm)

      // === MIDDLE: Recipient (large) ===
      grid(
        columns: (1fr, auto),
        [
          #text(size: 7pt, fill: accent, weight: "bold", tracking: 0.05em)[#upper(l("to", "TO"))]
          #v(0.15cm)
          #address-block(recipient, size: 14pt)
        ],
        // Logo if brand
        if has-brand {
          let logo = get(data, "brand.logos.primary", default: none)
          if logo != none and logo != "" {
            pad(left: 10pt, image(logo, width: 3cm))
          }
        },
      )

      v(1fr)

      // === BOTTOM: Shipment details ===
      line(length: 100%, stroke: 0.5pt + muted.transparentize(50%))
      v(0.15cm)

      grid(
        columns: (1fr, 1fr, 1fr, 1fr),
        column-gutter: 8pt,
        [
          #text(size: 7pt, fill: muted)[#l("tracking", "Tracking")]
          #linebreak()
          #text(size: 9pt, weight: "bold")[
            #get(data, "shipment.tracking", default: "")
          ]
        ],
        [
          #text(size: 7pt, fill: muted)[#l("weight", "Weight")]
          #linebreak()
          #text(size: 9pt, weight: "bold")[
            #get(data, "shipment.weight", default: "")
          ]
        ],
        [
          #text(size: 7pt, fill: muted)[#l("service", "Service")]
          #linebreak()
          #text(size: 9pt, weight: "bold")[
            #get(data, "shipment.service", default: "")
          ]
        ],
        [
          #text(size: 7pt, fill: muted)[#l("date", "Date")]
          #linebreak()
          #text(size: 9pt, weight: "bold")[
            #get(data, "shipment.date", default: "")
          ]
        ],
      )

      // Special instructions
      {
        let instructions = get(data, "shipment.instructions", default: "")
        if instructions != "" [
          #v(0.15cm)
          #rect(
            width: 100%,
            fill: accent.transparentize(92%),
            stroke: 0.5pt + accent,
            inset: 6pt,
            radius: 2pt,
          )[
            #text(size: 8pt, weight: "bold", fill: accent)[#upper(l("instructions", "SPECIAL INSTRUCTIONS"))]
            #linebreak()
            #text(size: 9pt)[#instructions]
          ]
        ]
      }
    })
  })
]

// Second label on same page if needed
#let extra-labels = {
  let v = get(data, "extra_copies", default: 0)
  if type(v) == str { int(v) } else if type(v) == int { v } else { 0 }
}
#if extra-labels > 0 [
  #v(1cm)
  #align(center)[
    #text(size: 7pt, fill: muted)[--- cut here ---]
  ]
  #v(0.5cm)
  // Repeat is handled by user printing multiple copies
]
