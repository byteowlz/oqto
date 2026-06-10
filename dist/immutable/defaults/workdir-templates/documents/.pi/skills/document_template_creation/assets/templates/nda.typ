// =============================================================================
// Non-Disclosure Agreement Template
// @description: Standard mutual or one-way NDA with signature blocks
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
#let muted = rgb("#718096")

// Data
#let agreement = get(data, "agreement", default: (:))
#let party_a = get(data, "party_a", default: (:))
#let party_b = get(data, "party_b", default: (:))
#let clauses = ensure-array(get(data, "clauses", default: ()))

// Labels
#let labels = get(data, "labels", default: (:))
#let l(key, fallback) = {
  if type(labels) == dictionary { labels.at(key, default: fallback) } else { fallback }
}

// Helper: party name
#let party-name(party) = {
  if type(party) != dictionary { return "Party" }
  let name = party.at("name", default: "")
  let company = party.at("company", default: "")
  if company != "" { company } else { name }
}

// Helper: party full block
#let party-block(party) = {
  if type(party) != dictionary { return [] }
  let name = party.at("name", default: "")
  let company = party.at("company", default: "")
  let address = party.at("address", default: "")
  let role = party.at("role", default: "")

  if company != "" { text(weight: "bold")[#company]; linebreak() }
  if name != "" and company != "" { [represented by #name]; linebreak() }
  else if name != "" { text(weight: "bold")[#name]; linebreak() }
  if address != "" { text(fill: muted)[#address]; linebreak() }
  if role != "" { text(fill: muted, size: 9pt)[(#role)] }
}

// Agreement config
#let agreement-type = get(data, "agreement.type", default: "mutual")
#let agreement-date = get(data, "agreement.date", default: "")
#let agreement-duration = str(get(data, "agreement.duration", default: "2 years"))
#let agreement-jurisdiction = str(get(data, "agreement.jurisdiction", default: ""))
#let agreement-purpose = str(get(data, "agreement.purpose", default: "exploring a potential business relationship"))

// Page setup
#set page(
  paper: "a4",
  margin: (top: 2.5cm, bottom: 2cm, left: 2.5cm, right: 2.5cm),
  header: if has-brand {
    let logo = get(data, "brand.logos.primary", default: none)
    if logo != none and logo != "" {
      align(right, image(logo, height: 1cm))
    }
  },
  footer: [
    #line(length: 100%, stroke: 0.3pt + muted)
    #v(0.15cm)
    #grid(
      columns: (1fr, 1fr, 1fr),
      text(size: 7pt, fill: muted)[NDA -- #party-name(party_a) / #party-name(party_b)],
      align(center, text(size: 7pt, fill: muted)[#l("confidential", "CONFIDENTIAL")]),
      align(right, text(size: 7pt, fill: muted)[Page #context counter(page).display()]),
    )
  ],
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Libertinus Serif") } else { "Libertinus Serif" },
  size: 10.5pt,
  fill: primary,
)
#set par(leading: 0.65em, justify: true)

// Clause numbering
#let clause-counter = counter("clause")


// =============================================================================
// DOCUMENT
// =============================================================================

#align(center)[
  #text(size: 18pt, weight: "bold", fill: primary, tracking: 0.05em)[
    #upper(l("title", "Non-Disclosure Agreement"))
  ]
  #v(0.1cm)
  #if agreement-type == "mutual" [
    #text(size: 10pt, fill: muted)[(#l("mutual", "Mutual"))]
  ] else [
    #text(size: 10pt, fill: muted)[(#l("unilateral", "Unilateral"))]
  ]
]

#v(0.6cm)

// Preamble
This Non-Disclosure Agreement (the *"Agreement"*) is entered into as of
#if agreement-date != "" [*#agreement-date*] else [the date of the last signature below]
by and between:

#v(0.3cm)

#grid(
  columns: (1fr, auto, 1fr),
  column-gutter: 0.5cm,
  rect(
    width: 100%, inset: 10pt, stroke: 0.5pt + muted.transparentize(40%), radius: 2pt,
  )[
    #text(size: 8pt, fill: muted, weight: "bold")[#upper(l("party_a", "PARTY A"))]
    #v(0.1cm)
    #party-block(party_a)
  ],
  align(center + horizon, text(size: 10pt, fill: muted)[and]),
  rect(
    width: 100%, inset: 10pt, stroke: 0.5pt + muted.transparentize(40%), radius: 2pt,
  )[
    #text(size: 8pt, fill: muted, weight: "bold")[#upper(l("party_b", "PARTY B"))]
    #v(0.1cm)
    #party-block(party_b)
  ],
)

#v(0.3cm)

(each a *"Party"* and collectively the *"Parties"*)

#v(0.2cm)

*WHEREAS*, the Parties wish to explore #agreement-purpose,
and in connection therewith may disclose to each other certain confidential
and proprietary information; and

*WHEREAS*, the Parties desire to establish the terms and conditions under
which such information will be disclosed and protected;

*NOW, THEREFORE*, in consideration of the mutual covenants and agreements
set forth herein, the Parties agree as follows:

#v(0.3cm)

// =============================================================================
// CLAUSES
// =============================================================================

// Default clauses if none provided
#let effective-clauses = if clauses.len() == 0 {(
  (
    title: "Definition of Confidential Information",
    text: "\"Confidential Information\" means any information, whether written, oral, electronic, or visual, disclosed by either Party to the other that is designated as confidential or that reasonably should be understood to be confidential given the nature of the information and the circumstances of disclosure.",
  ),
  (
    title: "Obligations",
    text: "The receiving Party shall: (a) hold the Confidential Information in strict confidence; (b) not disclose it to any third party without the prior written consent of the disclosing Party; (c) use it solely for the purpose described above; and (d) protect it using at least the same degree of care used to protect its own confidential information, but no less than reasonable care.",
  ),
  (
    title: "Exclusions",
    text: "Confidential Information does not include information that: (a) is or becomes publicly known through no fault of the receiving Party; (b) was known to the receiving Party prior to disclosure; (c) is independently developed without use of the Confidential Information; or (d) is rightfully obtained from a third party without restriction.",
  ),
  (
    title: "Term",
    text: "This Agreement shall remain in effect for " + agreement-duration + " from the date of execution. The obligations of confidentiality shall survive termination for a period of " + agreement-duration + " thereafter.",
  ),
  (
    title: "Return of Materials",
    text: "Upon termination of this Agreement or upon request, the receiving Party shall promptly return or destroy all copies of the Confidential Information and certify such destruction in writing.",
  ),
  (
    title: "No License",
    text: "Nothing in this Agreement grants any license or right under any patent, copyright, trademark, or other intellectual property right of either Party.",
  ),
  (
    title: "Remedies",
    text: "The Parties acknowledge that a breach of this Agreement may cause irreparable harm for which monetary damages may be inadequate. Accordingly, the disclosing Party shall be entitled to seek injunctive relief in addition to any other remedies available at law or in equity.",
  ),
  (
    title: "Governing Law",
    text: if agreement-jurisdiction != "" { "This Agreement shall be governed by and construed in accordance with the laws of " + agreement-jurisdiction + "." } else { "This Agreement shall be governed by and construed in accordance with the applicable laws of the jurisdiction agreed upon by the Parties." },
  ),
  (
    title: "Entire Agreement",
    text: "This Agreement constitutes the entire agreement between the Parties concerning the subject matter hereof and supersedes all prior agreements, understandings, and communications.",
  ),
)} else { clauses }

#for (i, clause) in effective-clauses.enumerate() [
  #let c-title = if type(clause) == dictionary { clause.at("title", default: "") } else { "" }
  #let c-text = if type(clause) == dictionary { clause.at("text", default: "") } else { str(clause) }

  *#(i + 1). #c-title.*
  #c-text

  #v(0.2cm)
]


// =============================================================================
// SIGNATURES
// =============================================================================

#v(1cm)

*IN WITNESS WHEREOF*, the Parties have executed this Agreement as of the date set forth above.

#v(1cm)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 2cm,

  // Party A
  [
    #text(size: 8pt, fill: muted, weight: "bold")[#upper(l("party_a", "PARTY A"))]
    #v(1.2cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Signature]
    #v(0.6cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Name: #if type(party_a) == dictionary { party_a.at("name", default: "") }]
    #v(0.6cm)
    #line(length: 60%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Date]
  ],

  // Party B
  [
    #text(size: 8pt, fill: muted, weight: "bold")[#upper(l("party_b", "PARTY B"))]
    #v(1.2cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Signature]
    #v(0.6cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Name: #if type(party_b) == dictionary { party_b.at("name", default: "") }]
    #v(0.6cm)
    #line(length: 60%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Date]
  ],
)
