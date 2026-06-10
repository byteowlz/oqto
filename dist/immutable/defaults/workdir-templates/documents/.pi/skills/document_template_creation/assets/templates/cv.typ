// =============================================================================
// CV / Resume Template
// @description: Clean one-page resume with sidebar layout for contact and skills
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color

#let data = tmpltr-data()
#let has-brand = "brand" in data and data.brand != none
#let ensure-array(val) = if type(val) == array { val } else { () }

// Colors
#let primary = if has-brand {
  rgb(brand-color(data, "primary", default: "#2d3748"))
} else {
  rgb(get(data, "style.primary_color", default: "#2d3748"))
}
#let accent = rgb(get(data, "style.accent_color", default: "#3182ce"))
#let muted = rgb("#718096")
#let sidebar-bg = rgb(get(data, "style.sidebar_color", default: "#f7fafc"))

// Data
#let person = get(data, "person", default: (:))
#let contact = get(data, "contact", default: (:))
#let skills = ensure-array(get(data, "skills", default: ()))
#let languages = ensure-array(get(data, "languages", default: ()))
#let experience = ensure-array(get(data, "experience", default: ()))
#let education = ensure-array(get(data, "education", default: ()))
#let certifications = ensure-array(get(data, "certifications", default: ()))
#let interests = ensure-array(get(data, "interests", default: ()))

// Labels
#let labels = get(data, "labels", default: (:))
#let l(key, fallback) = {
  if type(labels) == dictionary { labels.at(key, default: fallback) } else { fallback }
}

// Page setup
#set page(
  paper: "a4",
  margin: 0pt,
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Inter") } else { "Inter" },
  size: 9.5pt,
  fill: primary,
)
#set par(leading: 0.55em)


// =============================================================================
// LAYOUT: Sidebar + Main
// =============================================================================

#let sidebar-width = 6.5cm

// --- Helper: section heading ---
#let section-heading(title, color: primary) = {
  text(size: 11pt, weight: "bold", fill: color, tracking: 0.05em)[#upper(title)]
  v(0.15cm)
  line(length: 2cm, stroke: 1.5pt + color)
  v(0.25cm)
}

// --- Helper: skill bar ---
#let skill-bar(name, level: 3, max-level: 5) = {
  grid(
    columns: (1fr, auto),
    text(size: 9pt)[#name],
    {
      for i in range(max-level) {
        if i < level {
          box(width: 8pt, height: 8pt, baseline: 1pt,
            circle(radius: 3.5pt, fill: accent, stroke: none))
        } else {
          box(width: 8pt, height: 8pt, baseline: 1pt,
            circle(radius: 3.5pt, fill: none, stroke: 0.5pt + accent.transparentize(50%)))
        }
        if i < max-level - 1 { h(2pt) }
      }
    },
  )
  v(0.1cm)
}

// Full page grid
#grid(
  columns: (sidebar-width, 1fr),

  // =====================================================================
  // SIDEBAR
  // =====================================================================
  rect(width: 100%, height: 100%, fill: sidebar-bg, stroke: none, inset: 0pt)[
    #pad(top: 1.5cm, left: 1cm, right: 0.8cm, bottom: 1cm)[

      // Photo placeholder (optional)
      #let photo = get(data, "person.photo", default: "")
      #if photo != "" [
        #align(center)[
          #box(
            clip: true,
            radius: 50%,
            width: 3.5cm,
            height: 3.5cm,
            image(photo, width: 3.5cm),
          )
        ]
        #v(0.5cm)
      ]

      // Contact
      #section-heading(l("contact", "Contact"), color: accent)

      #let contact-items = (
        ("email", get(data, "contact.email", default: "")),
        ("phone", get(data, "contact.phone", default: "")),
        ("location", get(data, "contact.location", default: "")),
        ("website", get(data, "contact.website", default: "")),
        ("linkedin", get(data, "contact.linkedin", default: "")),
        ("github", get(data, "contact.github", default: "")),
      )

      #for (label, value) in contact-items [
        #if value != "" [
          #text(size: 8pt, fill: muted, weight: "bold")[#upper(label)]
          #linebreak()
          #text(size: 9pt)[#value]
          #v(0.2cm)
        ]
      ]

      #v(0.3cm)

      // Skills
      #if skills.len() > 0 [
        #section-heading(l("skills", "Skills"), color: accent)
        #for s in skills [
          #{
            let s-name = if type(s) == dictionary { s.at("name", default: "") } else { str(s) }
            let s-level = if type(s) == dictionary { s.at("level", default: 3) } else { 3 }
            skill-bar(s-name, level: s-level)
          }
        ]
        #v(0.3cm)
      ]

      // Languages
      #if languages.len() > 0 [
        #section-heading(l("languages", "Languages"), color: accent)
        #for lang in languages [
          #{
            let l-name = if type(lang) == dictionary { lang.at("name", default: "") } else { str(lang) }
            let l-level = if type(lang) == dictionary { lang.at("level", default: "") } else { "" }
            [*#l-name* #if l-level != "" [ -- #text(fill: muted)[#l-level] ] \ ]
          }
        ]
        #v(0.3cm)
      ]

      // Interests
      #if interests.len() > 0 [
        #section-heading(l("interests", "Interests"), color: accent)
        #let interest-texts = interests.map(i => {
          if type(i) == dictionary { i.at("name", default: "") } else { str(i) }
        })
        #text(size: 9pt)[#interest-texts.join(" | ")]
      ]
    ]
  ],

  // =====================================================================
  // MAIN CONTENT
  // =====================================================================
  rect(width: 100%, height: 100%, fill: white, stroke: none, inset: 0pt)[
    #pad(top: 1.5cm, left: 1.2cm, right: 1.5cm, bottom: 1cm)[

      // Name + Title
      #text(size: 28pt, weight: "bold", fill: primary)[
        #get(data, "person.name", default: "Your Name")
      ]
      #v(0.1cm)
      #text(size: 14pt, fill: accent)[
        #get(data, "person.title", default: "Professional Title")
      ]

      // Summary
      #let summary = get(data, "person.summary", default: "")
      #if summary != "" [
        #v(0.3cm)
        #text(size: 9.5pt, fill: muted)[#summary]
      ]

      #v(0.5cm)

      // Experience
      #if experience.len() > 0 [
        #section-heading(l("experience", "Experience"))

        #for (i, exp) in experience.enumerate() [
          #let exp-title = if type(exp) == dictionary { exp.at("title", default: "") } else { "" }
          #let exp-company = if type(exp) == dictionary { exp.at("company", default: "") } else { "" }
          #let exp-period = if type(exp) == dictionary { exp.at("period", default: "") } else { "" }
          #let exp-location = if type(exp) == dictionary { exp.at("location", default: "") } else { "" }
          #let exp-description = if type(exp) == dictionary { exp.at("description", default: "") } else { "" }
          #let exp-highlights = if type(exp) == dictionary { exp.at("highlights", default: ()) } else { () }
          #let exp-highlights = if type(exp-highlights) == array { exp-highlights } else { () }

          #grid(
            columns: (1fr, auto),
            [
              #text(size: 11pt, weight: "bold")[#exp-title]
              #linebreak()
              #text(size: 10pt, fill: accent)[#exp-company]
              #if exp-location != "" [ #text(size: 9pt, fill: muted)[ | #exp-location] ]
            ],
            align(right, text(size: 9pt, fill: muted)[#exp-period]),
          )
          #if exp-description != "" [
            #v(0.1cm)
            #text(size: 9pt)[#exp-description]
          ]
          #if exp-highlights.len() > 0 [
            #v(0.1cm)
            #for h in exp-highlights [
              #text(size: 9pt)[#text(fill: accent)[--] #if type(h) == dictionary { h.at("text", default: "") } else { str(h) }]
              #linebreak()
            ]
          ]
          #if i < experience.len() - 1 [ #v(0.35cm) ]
        ]
        #v(0.5cm)
      ]

      // Education
      #if education.len() > 0 [
        #section-heading(l("education", "Education"))

        #for (i, edu) in education.enumerate() [
          #let edu-degree = if type(edu) == dictionary { edu.at("degree", default: "") } else { "" }
          #let edu-school = if type(edu) == dictionary { edu.at("school", default: "") } else { str(edu) }
          #let edu-period = if type(edu) == dictionary { edu.at("period", default: "") } else { "" }
          #let edu-details = if type(edu) == dictionary { edu.at("details", default: "") } else { "" }

          #grid(
            columns: (1fr, auto),
            [
              #text(size: 10pt, weight: "bold")[#edu-degree]
              #linebreak()
              #text(size: 9.5pt, fill: accent)[#edu-school]
            ],
            align(right, text(size: 9pt, fill: muted)[#edu-period]),
          )
          #if edu-details != "" [
            #v(0.05cm)
            #text(size: 9pt, fill: muted)[#edu-details]
          ]
          #if i < education.len() - 1 [ #v(0.25cm) ]
        ]
        #v(0.5cm)
      ]

      // Certifications
      #if certifications.len() > 0 [
        #section-heading(l("certifications", "Certifications"))
        #for cert in certifications [
          #{
            let c-name = if type(cert) == dictionary { cert.at("name", default: "") } else { str(cert) }
            let c-issuer = if type(cert) == dictionary { cert.at("issuer", default: "") } else { "" }
            let c-date = if type(cert) == dictionary { cert.at("date", default: "") } else { "" }
            [*#c-name* #if c-issuer != "" [-- #text(fill: muted)[#c-issuer]] #if c-date != "" [ #text(size: 8pt, fill: muted)[(#c-date)]] \ ]
          }
        ]
      ]
    ]
  ],
)
