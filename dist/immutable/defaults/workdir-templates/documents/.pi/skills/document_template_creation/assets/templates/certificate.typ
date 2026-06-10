// =============================================================================
// Certificate / Award Template
// @description: Professional certificate for awards, certifications, or recognition
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color

#let data = tmpltr-data()
#let cert = get(data, "certificate", default: (:))
#let has-brand = "brand" in data and data.brand != none

// Colors
#let gold = "#d4af37"
#let dark-blue = "#1e3a5f"
#let primary = if has-brand { 
  brand-color(data, "primary", default: dark-blue) 
} else { 
  get(data, "certificate.primary_color", default: dark-blue)
}

// Page setup - landscape for certificate
#set page(
  paper: "a4",
  flipped: true,  // Landscape
  margin: 1.5cm,
  background: [
    // Border frame
    #place(rect(
      width: 100%,
      height: 100%,
      stroke: (top: 4pt + rgb(primary), bottom: 4pt + rgb(primary), 
               left: 4pt + rgb(primary), right: 4pt + rgb(primary)),
    ))
    #place(dx: 0.3cm, dy: 0.3cm, rect(
      width: 100% - 0.6cm,
      height: 100% - 0.6cm,
      stroke: 1pt + rgb(primary + "60"),
    ))
  ]
)

// Typography
#set text(
  font: if has-brand { 
    get(data, "brand.fonts.body", default: "Crimson Text") 
  } else { 
    "Crimson Text" 
  },
  size: 14pt,
)

// =============================================================================
// CERTIFICATE CONTENT
// =============================================================================

#v(1.5cm)

// Organization header
#align(center)[
  #text(size: 12pt, fill: luma(100))[
    #get(data, "issuer.organization", default: "Organization Name")
  ]
  #if has-brand {
    v(0.5cm)
    image(get(data, "brand.logos.primary", default: ""), width: 3cm)
  }
]

#v(1cm)

// Certificate type
#align(center)[
  #text(size: 36pt, weight: "bold", fill: rgb(primary), tracking: 0.1em)[
    #get(data, "certificate.type", default: "CERTIFICATE")
  ]
  #v(0.3cm)
  #text(size: 24pt, style: "italic", fill: rgb(gold))[
    #get(data, "certificate.subtype", default: "of Achievement")
  ]
]

#v(1.2cm)

// Present statement
#align(center)[
  #text(size: 14pt, fill: luma(100))[
    This is to certify that
  ]
]

#v(0.5cm)

// Recipient name
#align(center)[
  #text(size: 42pt, weight: "bold", fill: rgb(primary))[
    #get(data, "recipient.name", default: "Recipient Name")
  ]
  #if get(data, "recipient.title", default: "") != "" [
    #v(0.3cm)
    #text(size: 16pt, fill: luma(100))[
      #get(data, "recipient.title", default: "")
    ]
  ]
]

#v(0.8cm)

// Achievement description
#align(center)[
  #box(width: 80%)[
    #text(size: 14pt)[
      #get(data, "certificate.description", default: "Has successfully completed the requirements and demonstrated outstanding proficiency in")
    ]
    #v(0.5cm)
    #text(size: 20pt, weight: "bold", fill: rgb(primary))[
      #get(data, "achievement.title", default: "Course or Achievement Title")
    ]
  ]
]

#v(0.8cm)

// Details (date, location)
#align(center)[
  #text(size: 12pt, fill: luma(100))[
    #get(data, "certificate.date_text", default: "Awarded on")
    #v(0.2cm)
    #text(size: 14pt, weight: "bold")[
      #get(data, "certificate.date", default: "January 1, 2025")
    ]
    #if get(data, "certificate.location", default: "") != "" [
      #v(0.2cm)
      #get(data, "certificate.location", default: "")
    ]
  ]
]

#v(1.5cm)

// Signatures
#let signers = get(data, "signatures", default: ())
#if signers.len() > 0 [
  #align(center)[
    #grid(
      columns: signers.len() * (1fr,),
      column-gutter: 2cm,
      ..signers.map(signer => {
        align(center)[
          #line(length: 5cm, stroke: 0.5pt + luma(150))
          #v(0.3cm)
          #text(size: 12pt, weight: "bold")[#signer.name]
          #v(0.1cm)
          #text(size: 10pt, fill: luma(100))[#signer.title]
          #if "organization" in signer [
            // newline with smaller org text
            #v(0.05cm)
            #text(size: 9pt, fill: luma(120))[#signer.organization]
          ]
        ]
      })
    )
  ]
]

#v(1cm)

// Certificate ID / verification
#align(center)[
  #text(size: 9pt, fill: luma(120))[
    #get(data, "certificate.id_label", default: "Certificate ID"): 
    #get(data, "certificate.id", default: "CERT-000000")
    #if get(data, "certificate.verify_url", default: "") != "" [
      | Verify: #get(data, "certificate.verify_url", default: "")
    ]
  ]
]
