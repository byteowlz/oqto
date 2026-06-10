// =============================================================================
// Service Contract Template
// @description: Professional service/freelance contract with terms, payment, and signatures
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
#let accent = rgb(get(data, "style.accent_color", default: "#2b6cb0"))
#let muted = rgb("#718096")

// Data
#let contract = get(data, "contract", default: (:))
#let client = get(data, "client", default: (:))
#let provider = get(data, "provider", default: (:))
#let scope = ensure-array(get(data, "scope", default: ()))
#let deliverables = ensure-array(get(data, "deliverables", default: ()))
#let payment = get(data, "payment", default: (:))
#let milestones = ensure-array(get(data, "milestones", default: ()))
#let clauses = ensure-array(get(data, "clauses", default: ()))

// Labels
#let labels = get(data, "labels", default: (:))
#let l(key, fallback) = {
  if type(labels) == dictionary { labels.at(key, default: fallback) } else { fallback }
}

// Helper
#let party-name(party) = {
  if type(party) != dictionary { return "Party" }
  let company = party.at("company", default: "")
  let name = party.at("name", default: "")
  if company != "" { company } else { name }
}

#let party-block(party) = {
  if type(party) != dictionary { return [] }
  let name = party.at("name", default: "")
  let company = party.at("company", default: "")
  let address = party.at("address", default: "")
  let email = party.at("email", default: "")
  let tax_id = party.at("tax_id", default: "")

  if company != "" { text(weight: "bold")[#company]; linebreak() }
  if name != "" { [#name]; linebreak() }
  if address != "" { text(fill: muted)[#address]; linebreak() }
  if email != "" { text(size: 9pt, fill: muted)[#email]; linebreak() }
  if tax_id != "" { text(size: 8pt, fill: muted)[Tax ID: #tax_id] }
}

// Contract config
#let contract-number = get(data, "contract.number", default: "")
#let contract-date = get(data, "contract.date", default: "")
#let contract-start = get(data, "contract.start_date", default: "")
#let contract-end = get(data, "contract.end_date", default: "")
#let contract-title = get(data, "contract.title", default: "Service Agreement")

// Page setup
#set page(
  paper: "a4",
  margin: (top: 2.5cm, bottom: 2cm, left: 2.5cm, right: 2.5cm),
  header: {
    grid(
      columns: (1fr, 1fr),
      {
        if has-brand {
          let logo = get(data, "brand.logos.primary", default: none)
          if logo != none and logo != "" {
            image(logo, height: 1cm)
          }
        }
      },
      align(right)[
        #if contract-number != "" {
          text(size: 8pt, fill: muted)[Contract \#: *#contract-number*]
        }
      ],
    )
  },
  footer: [
    #line(length: 100%, stroke: 0.3pt + muted)
    #v(0.1cm)
    #grid(
      columns: (1fr, 1fr, 1fr),
      text(size: 7pt, fill: muted)[#party-name(provider) / #party-name(client)],
      align(center, text(size: 7pt, fill: muted)[#contract-title]),
      align(right, text(size: 7pt, fill: muted)[Page #context counter(page).display() of #context counter(page).final().first()]),
    )
  ],
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Libertinus Serif") } else { "Libertinus Serif" },
  size: 10.5pt,
  fill: primary,
)
#set par(leading: 0.65em, justify: true)

// Section heading helper
#let section(num, title) = {
  v(0.4cm)
  text(size: 12pt, weight: "bold", fill: primary)[#num. #title]
  v(0.15cm)
}


// =============================================================================
// DOCUMENT
// =============================================================================

#align(center)[
  #text(size: 20pt, weight: "bold", fill: primary, tracking: 0.03em)[
    #upper(contract-title)
  ]
  #v(0.1cm)
  #if contract-date != "" [
    #text(size: 10pt, fill: muted)[#contract-date]
  ]
]

#v(0.6cm)

// Parties
This Agreement (the *"Contract"*) is entered into by and between:

#v(0.3cm)

#grid(
  columns: (1fr, auto, 1fr),
  column-gutter: 0.5cm,
  rect(width: 100%, inset: 10pt, stroke: 0.5pt + muted.transparentize(40%), radius: 2pt)[
    #text(size: 8pt, fill: accent, weight: "bold")[#upper(l("provider", "SERVICE PROVIDER"))]
    #v(0.1cm)
    #party-block(provider)
  ],
  align(center + horizon, text(size: 10pt, fill: muted)[and]),
  rect(width: 100%, inset: 10pt, stroke: 0.5pt + muted.transparentize(40%), radius: 2pt)[
    #text(size: 8pt, fill: accent, weight: "bold")[#upper(l("client", "CLIENT"))]
    #v(0.1cm)
    #party-block(client)
  ],
)

#v(0.3cm)

(hereinafter referred to as *"Provider"* and *"Client"* respectively, and collectively as the *"Parties"*)


// --- 1. Scope of Services ---
#section(1, l("scope_title", "Scope of Services"))

The Provider agrees to perform the following services for the Client:

#if scope.len() > 0 [
  #for (i, s) in scope.enumerate() [
    #let s-text = if type(s) == dictionary { s.at("text", default: "") } else { str(s) }
    #text(size: 10pt)[#(i + 1). #s-text]
    #linebreak()
  ]
] else [
  #text(fill: muted)[\[Services to be defined\]]
]


// --- 2. Deliverables ---
#if deliverables.len() > 0 [
  #section(2, l("deliverables_title", "Deliverables"))

  #table(
    columns: (auto, 1fr, auto, auto),
    stroke: 0.5pt + muted.transparentize(40%),
    inset: 8pt,
    fill: (_, row) => if row == 0 { primary.transparentize(92%) } else { none },

    text(size: 9pt, weight: "bold")[\#],
    text(size: 9pt, weight: "bold")[#l("deliverable", "Deliverable")],
    text(size: 9pt, weight: "bold")[#l("due_date", "Due Date")],
    text(size: 9pt, weight: "bold")[#l("status", "Status")],

    ..deliverables.enumerate().map(((i, d)) => {
      let d-text = if type(d) == dictionary { d.at("text", default: "") } else { str(d) }
      let d-due = if type(d) == dictionary { d.at("due", default: "") } else { "" }
      let d-status = if type(d) == dictionary { d.at("status", default: "pending") } else { "pending" }
      (
        text(size: 9pt)[#(i + 1)],
        [#d-text],
        text(size: 9pt)[#d-due],
        text(size: 9pt)[#d-status],
      )
    }).flatten(),
  )
]


// --- 3. Term ---
#let term-section-num = if deliverables.len() > 0 { 3 } else { 2 }
#section(term-section-num, l("term_title", "Term"))

This Contract shall commence on
#if contract-start != "" [*#contract-start*] else [the date of execution]
and shall continue until
#if contract-end != "" [*#contract-end*] else [the completion of all deliverables],
unless earlier terminated in accordance with this Contract.


// --- 4. Compensation ---
#let pay-section-num = term-section-num + 1
#section(pay-section-num, l("payment_title", "Compensation and Payment"))

#let total = get(data, "payment.total", default: "")
#let rate = get(data, "payment.rate", default: "")
#let currency = get(data, "payment.currency", default: "EUR")
#let terms = get(data, "payment.terms", default: "30 days")
#let method = get(data, "payment.method", default: "bank transfer")

#if total != "" [
  The Client shall pay the Provider a total fee of *#currency #total* for the services described herein.
] else if rate != "" [
  The Client shall pay the Provider at a rate of *#currency #rate* per hour for the services described herein.
] else [
  The compensation for services shall be as agreed upon by the Parties.
]

Payment shall be due within *#terms* of invoice date and shall be made via *#method*.

#if milestones.len() > 0 [
  #v(0.2cm)
  Payment shall be made according to the following milestones:

  #table(
    columns: (auto, 1fr, auto, auto),
    stroke: 0.5pt + muted.transparentize(40%),
    inset: 8pt,
    fill: (_, row) => if row == 0 { primary.transparentize(92%) } else { none },

    text(size: 9pt, weight: "bold")[\#],
    text(size: 9pt, weight: "bold")[#l("milestone", "Milestone")],
    text(size: 9pt, weight: "bold")[#l("amount", "Amount")],
    text(size: 9pt, weight: "bold")[#l("due", "Due")],

    ..milestones.enumerate().map(((i, m)) => {
      let m-text = if type(m) == dictionary { m.at("text", default: "") } else { str(m) }
      let m-amount = if type(m) == dictionary { m.at("amount", default: "") } else { "" }
      let m-due = if type(m) == dictionary { m.at("due", default: "") } else { "" }
      (
        text(size: 9pt)[#(i + 1)],
        [#m-text],
        text(size: 9pt)[#currency #m-amount],
        text(size: 9pt)[#m-due],
      )
    }).flatten(),
  )
]


// --- Additional clauses ---
#let next-num = pay-section-num + 1

// Default clauses
#let effective-clauses = if clauses.len() == 0 {(
  (
    title: "Intellectual Property",
    text: "All work product created by the Provider under this Contract shall become the property of the Client upon full payment. The Provider retains the right to use general knowledge, skills, and experience gained during the engagement.",
  ),
  (
    title: "Confidentiality",
    text: "Both Parties agree to maintain the confidentiality of any proprietary or sensitive information exchanged during the course of this engagement. This obligation survives termination of this Contract.",
  ),
  (
    title: "Termination",
    text: "Either Party may terminate this Contract with 30 days' written notice. In the event of termination, the Client shall pay for all services rendered and expenses incurred up to the date of termination.",
  ),
  (
    title: "Liability",
    text: "The Provider's total liability under this Contract shall not exceed the total fees paid or payable under this Contract. Neither Party shall be liable for indirect, incidental, or consequential damages.",
  ),
  (
    title: "Governing Law",
    text: "This Contract shall be governed by and construed in accordance with the laws of " + get(data, "contract.jurisdiction", default: "the agreed jurisdiction") + ".",
  ),
  (
    title: "Entire Agreement",
    text: "This Contract constitutes the entire agreement between the Parties and supersedes all prior agreements and understandings. Amendments must be in writing and signed by both Parties.",
  ),
)} else { clauses }

#for (i, clause) in effective-clauses.enumerate() [
  #let c-title = if type(clause) == dictionary { clause.at("title", default: "") } else { "" }
  #let c-text = if type(clause) == dictionary { clause.at("text", default: "") } else { str(clause) }

  #section(next-num + i, c-title)
  #c-text
]


// =============================================================================
// SIGNATURES
// =============================================================================

#v(1cm)

*IN WITNESS WHEREOF*, the Parties have executed this Contract as of the date first written above.

#v(1cm)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 2cm,

  // Provider
  [
    #text(size: 8pt, fill: accent, weight: "bold")[#upper(l("provider", "SERVICE PROVIDER"))]
    #v(0.1cm)
    #text(size: 9pt)[#party-name(provider)]
    #v(1cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Signature]
    #v(0.6cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Name: #if type(provider) == dictionary { provider.at("name", default: "") }]
    #v(0.6cm)
    #line(length: 60%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Date]
  ],

  // Client
  [
    #text(size: 8pt, fill: accent, weight: "bold")[#upper(l("client", "CLIENT"))]
    #v(0.1cm)
    #text(size: 9pt)[#party-name(client)]
    #v(1cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Signature]
    #v(0.6cm)
    #line(length: 100%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Name: #if type(client) == dictionary { client.at("name", default: "") }]
    #v(0.6cm)
    #line(length: 60%, stroke: 0.5pt + primary)
    #v(0.1cm)
    #text(size: 9pt)[Date]
  ],
)
