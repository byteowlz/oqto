// Simple Quote Template
// @description: A clean, professional quote template for services or products
// @version: 1.1.0
// Usage: tmpltr compile simple-quote.toml [--brand <brand-id>]

// Import tmpltr helper library
#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-contact, brand-logo, brand-logo-image, brand-color

// Read data from tmpltr CLI
#let data = tmpltr-data()
#let quote = data.quote
#let blocks = data.blocks
#let has-brand = "brand" in data and data.brand != none

// Company info: prefer brand data if available, fall back to quote.company
#let company = (
  name: if has-brand { brand-contact(data, "company", default: quote.company.at("name", default: "")) } else { quote.company.at("name", default: "") },
  address: if has-brand { brand-contact(data, "address", default: quote.company.at("address", default: "")) } else { quote.company.at("address", default: "") },
  city: if has-brand { brand-contact(data, "city", default: quote.company.at("city", default: "")) } else { quote.company.at("city", default: "") },
  email: if has-brand { brand-contact(data, "email", default: quote.company.at("email", default: "")) } else { quote.company.at("email", default: "") },
  phone: if has-brand { brand-contact(data, "phone", default: quote.company.at("phone", default: "")) } else { quote.company.at("phone", default: "") },
  website: if has-brand { brand-contact(data, "website", default: quote.company.at("website", default: none)) } else { quote.company.at("website", default: none) },
  logo: if has-brand { brand-logo(data, variant: "primary", default: quote.company.at("logo", default: none)) } else { quote.company.at("logo", default: none) },
)

// Helper function to render a content block
#let render-block(block) = {
  if block.at("type", default: "text") == "table" {
    table(
      columns: block.columns.len() * (1fr,),
      stroke: 0.5pt,
      ..block.columns.map(c => [*#c*]),
      ..block.rows.flatten().map(cell => [#cell])
    )
  } else {
    eval(block.content, mode: "markup")
  }
}

// Helper to parse date string to datetime
#let parse-date(date-str) = {
  let parts = date-str.split("-")
  if parts.len() == 3 {
    datetime(
      year: int(parts.at(0)),
      month: int(parts.at(1)),
      day: int(parts.at(2))
    )
  } else {
    datetime.today()
  }
}

#let issue-date = parse-date(quote.date)
#let valid-until = parse-date(quote.valid_until)

// Page setup
#set page(
  paper: "a4",
  margin: (
    top: 2.5cm,
    bottom: 2cm,
    left: 2.5cm,
    right: 2cm
  ),
  numbering: "1",
  number-align: center + bottom,
)

// Text settings
#set text(
  font: "Helvetica Neue",
  size: 11pt,
  lang: "en"
)

#set par(
  justify: true,
  first-line-indent: 0pt,
  leading: 0.65em
)

// Heading styles
#set heading(numbering: "1.")

#show heading.where(level: 1): it => [
  #set text(size: 14pt, weight: "bold")
  #set par(first-line-indent: 0pt)
  #block(above: 1.5em, below: 1em)[
    #smallcaps[#it.body]
  ]
]

#show heading.where(level: 2): it => [
  #set text(size: 12pt, weight: "bold")
  #set par(first-line-indent: 0pt)
  #block(above: 1.2em, below: 0.8em)[
    #it.body
  ]
]

// Header with logo
#grid(
  columns: (1fr, auto),
  column-gutter: 1em,
  [
    #text(size: 18pt, weight: "bold")[#company.name]
    #v(0.3em)
    #text(size: 10pt)[
      #company.address \
      #company.city \
      #company.email \
      #company.phone
    ]
  ],
  [
    #if company.logo != none [
      #image(company.logo, width: 4cm)
    ]
  ]
)

#line(length: 100%, stroke: 1pt)

#v(1em)

// Quote title and number
#align(center)[
  #text(size: 20pt, weight: "bold")[QUOTE]
  #v(0.5em)
  #text(size: 12pt)[No. #quote.number]
  #v(0.5em)
  #text(size: 11pt)[Date: #issue-date.display("[month repr:long] [day], [year]")]
]

#v(1.5em)

// Client information
#rect(
  width: 100%,
  stroke: 0.5pt + gray,
  inset: 1em,
  fill: luma(245)
)[
  #text(weight: "bold")[Prepared for:]
  #v(0.5em)
  #quote.client.name \
  #quote.client.address \
  #quote.client.city \
  #if "email" in quote.client [#quote.client.email]
]

#v(1.5em)

// Introduction
#heading(level: 1)[Introduction]

Dear #quote.client.contact,

Thank you for your interest in our services. We are pleased to present you with this quote for the following project:

#v(0.5em)
#align(center)[
  #text(size: 12pt, style: "italic")[#quote.title]
]
#v(0.5em)

// Project description
#if "description" in blocks [
  #heading(level: 1)[#blocks.description.title]
  #render-block(blocks.description)
]

// Scope of work
#if "scope" in blocks [
  #heading(level: 1)[#blocks.scope.title]
  #render-block(blocks.scope)
]

// Deliverables
#if "deliverables" in blocks [
  #heading(level: 1)[#blocks.deliverables.title]
  #render-block(blocks.deliverables)
]

// Timeline
#if "timeline" in blocks [
  #heading(level: 1)[#blocks.timeline.title]
  #render-block(blocks.timeline)
]

// Pricing
#heading(level: 1)[Investment]

#table(
  columns: (2fr, 1fr),
  stroke: 0.5pt,
  fill: (x, y) => if y == 0 { luma(230) } else { none },
  [*Description*], [*Amount*],
  ..quote.line_items.map(item => (
    [#item.description],
    [#item.amount]
  )).flatten(),
  table.hline(stroke: 1pt),
  [*Total*], [*#quote.total*]
)

#v(0.5em)
#text(size: 10pt, style: "italic")[#quote.payment_terms]

// Terms and conditions
#if "terms" in blocks [
  #heading(level: 1)[#blocks.terms.title]
  #render-block(blocks.terms)
]

#v(2em)

// Validity
#rect(
  width: 100%,
  stroke: 1pt + gray,
  inset: 1em,
)[
  *Quote Validity:* This quote is valid until #valid-until.display("[month repr:long] [day], [year]").
]

#v(2em)

// Signature section
#grid(
  columns: (1fr, 1fr),
  column-gutter: 2em,
  [
    *Prepared by:*
    #v(2em)
    #line(length: 80%, stroke: 0.5pt)
    #quote.prepared_by \
    #company.name
  ],
  [
    *Accepted by:*
    #v(2em)
    #line(length: 80%, stroke: 0.5pt)
    #quote.client.contact \
    #quote.client.name
  ]
)

#v(1em)

#align(center)[
  #text(size: 10pt, fill: gray)[
    Questions? Contact us at #company.email or #company.phone
  ]
]
