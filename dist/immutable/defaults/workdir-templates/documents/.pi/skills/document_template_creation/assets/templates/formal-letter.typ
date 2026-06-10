// =============================================================================
// Formal Business Letter Template
// @description: Professional business letter with sender/recipient blocks
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color, brand-logo-image

#let data = tmpltr-data()
#let has-brand = "brand" in data and data.brand != none

// =============================================================================
// PAGE SETUP (DIN 5008 compliant)
// =============================================================================

#set page(
  paper: "a4",
  margin: (
    top: 2cm,      // DIN 5008: 2cm from top for letterhead
    bottom: 2cm,
    left: 2.5cm,
    right: 2cm,
  ),
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Helvetica Neue") } else { "Helvetica Neue" },
  size: 11pt,
  lang: get(data, "lang", default: "de"),
)

#set par(
  justify: true,
  leading: 1.2em,
  first-line-indent: 0pt,
)

#let primary-color = if has-brand { 
  brand-color(data, "primary", default: "#1e293b") 
} else { 
  "#1e293b" 
}

// =============================================================================
// HEADER / LETTERHEAD AREA
// =============================================================================

#place(
  top + left,
  dy: -0.5cm,
  [
    #if has-brand {
      brand-logo-image(data, width: get(data, "brand.logo-width", default: 4cm))
    }
  ]
)

#place(
  top + right,
  dy: -0.5cm,
  align(right)[
    #text(size: 9pt)[
      #get(data, "sender.company", default: "") \
      #get(data, "sender.street", default: "") \
      #get(data, "sender.postal", default: "") #get(data, "sender.city", default: "") \
      #v(0.5em)
      #get(data, "sender.phone", default: "") \
      #get(data, "sender.email", default: "") \
      #get(data, "sender.website", default: "")
    ]
  ]
)

#v(4cm)  // Space for letterhead

// =============================================================================
// SENDER (Return Address - Small Print)
// =============================================================================

#text(size: 8pt, fill: luma(100))[
  #get(data, "sender.company", default: "") | 
  #get(data, "sender.street", default: "") | 
  #get(data, "sender.postal", default: "") #get(data, "sender.city", default: "")
]

#v(0.5cm)

// =============================================================================
// RECIPIENT ADDRESS BLOCK
// =============================================================================

#block(width: 8cm)[
  #get(data, "recipient.company", default: "Empfängerfirma GmbH") \
  #get(data, "recipient.department", default: "") \
  #get(data, "recipient.contact", default: "z. Hd. Frau Dr. Schmidt") \
  #get(data, "recipient.street", default: "Musterstraße 123") \
  #get(data, "recipient.postal", default: "10115") #get(data, "recipient.city", default: "Berlin")
]

#v(1cm)

// =============================================================================
// METADATA (Date, Subject, Reference)
// =============================================================================

#grid(
  columns: (1fr, 1fr),
  column-gutter: 2em,
  [
    #get(data, "labels.location", default: "Berlin") , den 
    #get(data, "letter.date", default: "12. Februar 2025")
  ],
  align(right)[
    #let ref = get(data, "letter.reference", default: "")
    #if ref != "" [
      #get(data, "labels.your_ref", default: "Ihr Zeichen"): #ref
    ]
  ]
)

#v(1cm)

// =============================================================================
// SUBJECT LINE
// =============================================================================

#text(weight: "bold")[
  #get(data, "labels.subject", default: "Betreff"): 
  #get(data, "letter.subject", default: "Betreffzeile")
]

#v(0.8cm)

// =============================================================================
// SALUTATION
// =============================================================================

#get(data, "letter.salutation", default: "Sehr geehrte Damen und Herren,")

#v(0.5cm)

// =============================================================================
// BODY CONTENT
// =============================================================================

#get(data, "letter.body", default: "Hier steht der Text Ihres Geschäftsbriefs. Der Text sollte klar strukturiert und sachlich formuliert sein.")

#v(0.5cm)

// Optional additional paragraphs from blocks
#let paragraphs = get(data, "blocks.paragraphs", default: ())
#for para in paragraphs {
  para.content
  v(0.5cm)
}

// =============================================================================
// CLOSING
// =============================================================================

#v(0.5cm)

#get(data, "letter.closing", default: "Mit freundlichen Grüßen")

#v(1.5cm)

// =============================================================================
// SIGNATURE BLOCK
// =============================================================================

#grid(
  columns: (1fr,),
  [
    #line(length: 5cm, stroke: 0.5pt)
    #v(0.3cm)
    #get(data, "sender.name", default: "Max Mustermann") \
    #get(data, "sender.title", default: "") \
    #get(data, "sender.position", default: "Geschäftsführer")
  ]
)

#v(1cm)

// =============================================================================
// ATTACHMENTS & CC
// =============================================================================

#let attachments = get(data, "letter.attachments", default: ())
#let cc = get(data, "letter.cc", default: ())

#if attachments.len() > 0 or cc.len() > 0 [
  #v(1cm)
  #line(length: 100%, stroke: 0.5pt + luma(200))
  #v(0.5cm)
  
  #if attachments.len() > 0 [
    #text(size: 9pt)[
      #get(data, "labels.attachments", default: "Anlagen"): 
      #attachments.map(a => a.name).join(", ")
    ]
  ]
  
  #if cc.len() > 0 [
    #text(size: 9pt)[
      #get(data, "labels.cc", default: "Kopie an"): 
      #cc.map(c => c.name).join(", ")
    ]
  ]
]
