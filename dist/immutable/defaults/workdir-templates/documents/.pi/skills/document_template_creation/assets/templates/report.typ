// =============================================================================
// Professional Report Template
// @description: Business/technical report with cover page, TOC, and sections
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color, brand-logo-image

#let data = tmpltr-data()
#let report = get(data, "report", default: (:))
#let has-brand = "brand" in data and data.brand != none

// =============================================================================
// COLORS & STYLING
// =============================================================================

#let colors = (
  primary: if has-brand { brand-color(data, "primary", default: "#1e3a5f") } else { "#1e3a5f" },
  accent: if has-brand { brand-color(data, "accent", default: "#3b82f6") } else { "#3b82f6" },
  text: "#1f2937",
  light: "#f3f4f6",
)

#set page(
  paper: "a4",
  margin: (top: 2.5cm, bottom: 2cm, left: 3cm, right: 2.5cm),
  numbering: "1",
  number-align: center + bottom,
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Source Sans Pro") } else { "Source Sans Pro" },
  size: 11pt,
  fill: rgb(colors.text),
)

#set par(leading: 0.8em, justify: true)

#set heading(numbering: "1.1")

#show heading.where(level: 1): it => [
  #pagebreak()
  #v(1cm)
  #text(size: 24pt, weight: "bold", fill: rgb(colors.primary))[#it.body]
  #v(1cm)
]

#show heading.where(level: 2): it => [
  #v(1em)
  #text(size: 16pt, weight: "bold", fill: rgb(colors.primary))[#it.body]
  #v(0.5em)
]

#show heading.where(level: 3): it => [
  #v(0.5em)
  #text(size: 13pt, weight: "bold", fill: rgb(colors.text))[#it.body]
  #v(0.3em)
]

// =============================================================================
#let make-cover = {
  page(margin: 0cm)[
    // Background accent
    #place(
      top + right,
      rect(
        width: 8cm,
        height: 100%,
        fill: rgb(colors.primary + "20"),
      )
    )
    
    // Logo
    #place(
      top + left,
      dx: 2cm,
      dy: 2cm,
      if has-brand {
        brand-logo-image(data, width: 5cm)
      }
    )
    
    // Title block
    #place(
      center + horizon,
      dy: -3cm,
      [
        #align(left)[
          #text(size: 14pt, fill: rgb(colors.accent))[
            #get(data, "report.type", default: "Technical Report")
          ]
          #v(0.5cm)
          #text(size: 32pt, weight: "bold", fill: rgb(colors.primary))[
            #get(data, "report.title", default: "Report Title")
          ]
          #v(0.5cm)
          #text(size: 16pt, fill: luma(100))[
            #get(data, "report.subtitle", default: "")
          ]
        ]
      ]
    )
    
    // Metadata block
    #place(
      bottom + left,
      dx: 2cm,
      dy: -3cm,
      [
        #line(length: 10cm, stroke: 1pt + rgb(colors.primary))
        #v(0.5cm)
        #text(size: 12pt)[
          #grid(
            columns: (auto, 1fr),
            column-gutter: 1em,
            row-gutter: 0.5em,
            [**Author:**], [#get(data, "report.author", default: "Author Name")],
            [**Date:**], [#get(data, "report.date", default: "January 2025")],
            [**Version:**], [#get(data, "report.version", default: "1.0")],
            #if get(data, "report.client", default: "") != "" [
              (**Client:**)
              (#get(data, "report.client", default: ""))
            ],
          )
        ]
      ]
    )
  ]
}

// =============================================================================
// DOCUMENT START
// =============================================================================

#make-cover

// Table of Contents
#pagebreak()
#heading(numbering: none)[Table of Contents]
#outline(indent: 15pt, depth: 3)

// Executive Summary
#if get(data, "report.summary", default: "") != "" [
  #pagebreak()
  #heading(numbering: none)[Executive Summary]
  #get(data, "report.summary", default: "")
]

// =============================================================================
// MAIN CONTENT SECTIONS
// Load from blocks
// =============================================================================

#let sections = get(data, "sections", default: ())

#for section in sections {
  heading(level: 1)[#section.title]
  
  if "content" in section {
    section.content
  }
  
  // Subsections
  if "subsections" in section {
    for sub in section.subsections {
      heading(level: 2)[#sub.title]
      sub.content
    }
  }
}

// =============================================================================
// APPENDIX (if present)
// =============================================================================

#let appendix = get(data, "appendix", default: ())
#if appendix.len() > 0 [
  #pagebreak()
  #heading(numbering: none)[Appendix]
  
  #for item in appendix {
    heading(level: 2)[#item.title]
    item.content
  }
]

// =============================================================================
// FOOTER ON LAST PAGE
// =============================================================================

#pagebreak()
#align(center)[
  #text(size: 10pt, fill: luma(120))[
    #if has-brand {
      get(data, "brand.contact.company", default: "") + " | "
    }
    #get(data, "report.author", default: "") \
    #get(data, "report.date", default: "")
    
    #if get(data, "report.confidential", default: false) [
      #v(0.5em)
      *Confidential - Do Not Distribute*
    ]
  ]
]
