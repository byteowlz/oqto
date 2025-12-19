// Sample tmpltr template
// @description: Example template demonstrating tmpltr library usage
// @version: 1.0.0

// Import tmpltr helper library
#import "@local/tmpltr-lib:1.0.0": editable, editable-block, tmpltr-data, get, md, render-table

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, editable, editable-block, get, brand-color, brand-logo, brand-logo-image, brand-font
// Get data from tmpltr CLI
#let data = tmpltr-data()

// Page setup
#set page(
  paper: "a4",
  margin: 2.5cm,
)

#set text(
  font: "Helvetica Neue",
  size: 11pt,
  lang: "de"
)

// Title
#align(center)[
  #text(size: 16pt, weight: "bold")[
    #editable("quote.title", get(data, "quote.title", default: "Document Title"))
  ]
  #v(1em)
  Quote Number: #editable("quote.number", get(data, "quote.number", default: "2025-001"))
  #v(0.5em)
  Date: #editable("quote.date", get(data, "quote.date", default: "2025-01-01"))
]

#line(length: 100%)

// Client information
#heading(level: 1)[Client]

#editable("quote.client.name", get(data, "quote.client.name", default: "Client Name"))

#editable("quote.client.address", get(data, "quote.client.address", default: "Client Address"))

#v(1em)

// Introduction block
#heading(level: 1)[Introduction]

#editable-block("blocks.intro", title: "Introduction", format: "markdown")[
  #let intro = get(data, "blocks.intro.content", default: "Introduction content goes here...")
  #md(intro)
]

#v(1em)

// Scope block
#heading(level: 1)[Scope of Work]

#editable-block("blocks.scope", title: "Scope of Work", format: "markdown")[
  #let scope = get(data, "blocks.scope.content", default: "Scope description goes here...")
  #md(scope)
]

#v(1em)

// Timeline table
#heading(level: 1)[Timeline]

#editable-block("blocks.timeline", title: "Timeline", format: "table")[
  #let timeline = get(data, "blocks.timeline")
  #if timeline != none [
    #render-table(timeline)
  ] else [
    _Timeline to be defined_
  ]
]

#v(1em)

// Pricing
#heading(level: 1)[Pricing]

Total: #editable("quote.total", get(data, "quote.total", default: "0.00 EUR"))

#v(2em)

// Signature
#line(length: 50%)
Signature
