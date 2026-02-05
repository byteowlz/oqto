// byteowlz Angebot Template
// This template reads structured data passed by tmpltr CLI
// Usage: tmpltr compile content.toml

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-logo, brand-font, brand-color

// Read data from tmpltr CLI (passed as JSON string)
#let data = tmpltr-data()
#let quote = data.at("quote", default: (:))
#let blocks = data.at("blocks", default: (:))

// Brand configuration
#let logo-path = brand-logo(data, variant: "primary", default: none)
#let logo-width-str = get(data, "brand.logo-width", default: "4cm")
#let logo-width = if type(logo-width-str) == str { eval(logo-width-str) } else { 4cm }
#let title-logo-width-str = get(data, "brand.title-logo-width", default: "8cm")
#let title-logo-width = if type(title-logo-width-str) == str { eval(title-logo-width-str) } else { 8cm }
#let primary-font = brand-font(data, usage: "body", default: "Helvetica Neue")

// Company information from brand.contact
#let contact = data.at("brand", default: (:)).at("contact", default: (:))
#let company-name = contact.at("company", default: "Company Name")
#let company-legal-form = contact.at("legal-form", default: "")
#let company-full-name = if company-legal-form != "" { company-name + " " + company-legal-form } else { company-name }
#let company-street = contact.at("street", default: "")
#let company-postal-code = contact.at("postal-code", default: "")
#let company-city = contact.at("city", default: "")
#let company-postal-city = if company-postal-code != "" { company-postal-code + " " + company-city } else { company-city }
#let company-phone = contact.at("phone", default: "")
#let company-fax = contact.at("fax", default: "")
#let company-email = contact.at("email", default: "")
#let company-website = contact.at("website", default: "")
#let company-people = contact.at("people", default: ())

// Render logo helper (returns nothing if no logo)
#let render-logo(width) = {
  if logo-path != none {
    image(logo-path, width: width)
  }
}

// Helper function to render a content block
#let render-block(block) = {
  if block.at("type", default: "text") == "table" {
    // Render as table
    table(
      columns: block.columns.len() * (1fr,),
      stroke: 0.5pt,
      ..block.columns.map(c => [*#c*]),
      ..block.rows.flatten().map(cell => [#cell])
    )
  } else {
    // Render as text/markup
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

#let datum = parse-date(quote.projekt.datum)

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
  font: primary-font,
  size: 11pt,
  lang: "de"
)

#set par(
  justify: true,
  first-line-indent: 0pt,
  leading: 0.65em
)

// Heading styles
#set heading(numbering: "1.1")

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
    #smallcaps[#it.body]
  ]
]

// Title page
#align(center)[
  #text(size: 14pt)[Angebot Nr. #quote.angebot_nr]
  #v(1em)
  #text(size: 16pt, weight: "bold")[#smallcaps[#quote.angebot_titel]]
  #v(2em)
  
  #render-logo(title-logo-width)
  #v(1em)
  
  #company-street\
  #company-postal-city\
  \
  Telefon #company-phone
  
  #v(2em)
  
  Erstellt von\
  #quote.erstellt_von.personen.join(linebreak())
  
  #v(1em)
  #company-city, #datum.display("[day]. [month repr:long] [year]")
  
  #v(1em)
  #emph[für #quote.kunde.name, #quote.kunde.plz_ort]
]

#pagebreak()

// Address block
#rect(
  width: 100%,
  stroke: 0.5pt + black,
  inset: 1em
)[
  #text(size: 9pt)[#company-full-name | #company-street | #company-postal-city]
  #v(1em)
  
  #quote.kunde.name\
  #quote.kunde.strasse\
  #quote.kunde.plz_ort
]

#v(1em)

// Sender info box
#rect(
  width: 100%,
  stroke: 0.5pt + black,
  inset: 1em
)[
  #grid(
    columns: (1fr, auto),
    column-gutter: 1em,
    [
      #company-full-name\
      \
      #company-people.join(linebreak())\
      \
      #company-street\
      #company-postal-city\
      \
      #quote.erstellt_von.personen.at(0)\
      Telefon #company-phone#if company-fax != "" [ | Fax #company-fax]\
      #if company-email != "" [#link("mailto:" + company-email)[#company-email.replace("@", "\\@")]]\
      \
      #company-website
    ],
    [
      #render-logo(logo-width)
    ]
  )
]

#v(2em)

// Main offer content
#align(center)[
  #text(size: 14pt)[Angebot Nr. #quote.angebot_nr]
  #v(0.5em)
  #text(size: 16pt, weight: "bold")[#smallcaps[#quote.angebot_titel]]
  #v(1em)
  
  #line(length: 100%, stroke: 0.5pt)
  #company-city, #datum.display("[day]. [month repr:long] [year]")
  #line(length: 100%, stroke: 0.5pt)
]

#v(1.5em)

#quote.projekt.anrede #quote.projekt.ansprechpartner,

wir danken Ihnen für Ihr Interesse an einer Zusammenarbeit mit uns und erlauben uns Ihnen hiermit auf der Grundlage der von uns ausgearbeiteten Aufgabenbeschreibung:

#v(0.5em)
#align(center)[#emph[#quote.angebot_titel]]
#v(0.5em)

zu nachstehenden Konditionen anzubieten:

// Main sections
#heading(level: 1)[Gegenstand]

Durchführung des o.g. Vorhabens gemäß beigefügter Aufgabenbeschreibung (Anlage A).

#heading(level: 1)[Vorgesehene Bearbeitungsdauer, Termine]

Geplanter Starttermin des vorliegenden Projektvorhabens ist der #quote.projekt.projekt_start mit einer initialen Laufzeit von #quote.projekt.laufzeit und der Möglichkeit einer Verlängerung um jeweils weitere 6 Monate.

#heading(level: 1)[Vergütung]

Wir vereinbaren auf der Grundlage der in Anlage A beschriebenen Aufgaben und Nebenbedingungen eine feste Vergütung von #quote.projekt.verguetung zzgl. USt und Reisekosten nach Aufwand.

#heading(level: 1)[Zahlungsplan]

Die Vergütung zzgl. USt erfolgt nach folgendem Zahlungsplan und ist innerhalb von 10 Tagen netto nach Teilzahlungsdatum fällig.

#table(
  columns: (1fr, 1fr),
  stroke: 0.5pt,
  [*Teilzahlungen*], [*Zahlungsvolumen*],
  ..quote.zahlungsplan.enumerate().map(((i, entry)) => (
    [#(i+1). Teilzahlung zum #entry.datum],
    [#entry.betrag]
  )).flatten()
)

Reisekosten werden gesondert in Rechnung gestellt.

Alle Zahlungen sind abzugs- und spesenfrei auf das in der Rechnung bezeichnete Konto der #company-full-name zu veranlassen.

#heading(level: 1)[Sonstiges]

#heading(level: 2)[Import- und Exportkontrolle]

Die Vertragspartner verpflichten sich zur Einhaltung aller anwendbaren nationalen, europäischen, ausländischen und internationalen Vorschriften des Außenwirtschaftsrechts einschließlich Embargos (und/oder sonstigen Sanktionen).

Sollte die Leistungserbringung durch #company-name ausfallen oder sich verzögern und beruht dies auf einem außenwirtschaftsrechtlichen Verbot, auf der Nichterteilung einer erforderlichen außenwirtschaftsrechtlichen Genehmigung oder auf der Verzögerung des außenwirtschaftsrechtlichen behördlichen Genehmigungsverfahrens, ist eine Schadensersatzpflicht von #company-name ausgeschlossen.

#heading(level: 1)[Angebotsbestandteile]

Die beigefügte Aufgabenbeschreibung ist Bestandteil dieses Angebots.

#heading(level: 1)[Bindefrist]

An dieses Angebot halten wir uns bis zum #quote.projekt.bindefrist gebunden.

#v(1.5em)

Wir werden unsererseits eine effiziente Zusammenarbeit sicherstellen und erwarten gerne Ihren Auftrag.

Mit freundlichen Grüßen

#company-full-name

#v(2em)
M.Sc. #quote.erstellt_von.personen.at(0) #h(3cm) M.Sc. #quote.erstellt_von.personen.at(1)

#v(1em)
Anlagen: A -- Aufgabenbeschreibung

#pagebreak()

// Order confirmation box
#rect(
  width: 100%,
  stroke: 1pt + black,
  inset: 1em
)[
  Angebot Nr. #quote.angebot_nr
  
  Wir erteilen hiermit den Auftrag gemäß den Angebotsbedingungen mit folgender Bestellnummer:
  
  #v(1em)
  #text(style: "italic")[#box(width: 50%, stroke: (bottom: 0.5pt))[]] (falls vorhanden)
  
  #v(3em)
  #text(style: "italic")[
    #line(length: 100%, stroke: 0.5pt)\
    Datum, Firmenstempel, rechtsverbindliche Unterschrift
  ]
]

#pagebreak()

// Appendix A - Task Description
#align(center)[
  #text(size: 16pt, weight: "bold")[#smallcaps[Anlage A]]
  #v(0.5em)
  zu Angebot Nr. #quote.angebot_nr
  #v(1em)
  #text(size: 14pt, weight: "bold")[#smallcaps[Aufgabenbeschreibung]]
  #v(1em)
  für die Zusammenarbeit zwischen #quote.kunde.name, #quote.kunde.plz_ort und der #company-full-name in #company-city.
  #v(1em)
  #text(size: 16pt)[#quote.angebot_titel]
]

#v(2em)

// Render content blocks from TOML
#heading(level: 1)[Aufgaben, Ausgangssituation und Zielsetzung]

#if "ausgangssituation" in blocks.keys() [
  == #blocks.ausgangssituation.title
  #render-block(blocks.ausgangssituation)
]

#if "zielsetzung" in blocks.keys() [
  == #blocks.zielsetzung.title
  #render-block(blocks.zielsetzung)
]

#if "vorgehensweise" in blocks.keys() [
  #heading(level: 1)[#blocks.vorgehensweise.title]
  #render-block(blocks.vorgehensweise)
]

#if "ergebnisse" in blocks.keys() [
  #heading(level: 1)[#blocks.ergebnisse.title]
  #render-block(blocks.ergebnisse)
]

#if "zeitplan" in blocks.keys() [
  #heading(level: 1)[#blocks.zeitplan.title]
  #render-block(blocks.zeitplan)
]

#if "zusammenarbeit" in blocks.keys() [
  #heading(level: 1)[#blocks.zusammenarbeit.title]
  #render-block(blocks.zusammenarbeit)
]
