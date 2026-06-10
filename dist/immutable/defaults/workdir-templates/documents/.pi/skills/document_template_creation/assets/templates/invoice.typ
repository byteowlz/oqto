// =============================================================================
// Invoice Template
// @description: Professional invoice with payment terms and itemized billing
// @version: 1.0.0
// =============================================================================

#import "@local/tmpltr-lib:1.0.0": tmpltr-data, get, brand-color, brand-logo-image

#let data = tmpltr-data()
#let invoice = get(data, "invoice", default: (:))
#let has-brand = "brand" in data and data.brand != none

// =============================================================================
// HELPERS
// =============================================================================

#let format-currency(amount, currency) = {
  let symbol = if currency == "EUR" { "€" } 
    else if currency == "USD" { "$" }
    else if currency == "GBP" { "£" }
    else { currency }
  symbol + amount
}

#let today() = {
  let dt = datetime.today()
  dt.display("[day].[month].[year]")
}

// =============================================================================
// STYLING
// =============================================================================

#set page(
  paper: "a4",
  margin: (top: 2cm, bottom: 2cm, left: 2.5cm, right: 2.5cm),
)

#set text(
  font: if has-brand { get(data, "brand.fonts.body", default: "Helvetica Neue") } else { "Helvetica Neue" },
  size: 10pt,
)

#set par(leading: 0.6em)

// Colors - use rgb constructor directly
#let primary-color = rgb(if has-brand { 
  brand-color(data, "primary", default: "#1e293b") 
} else { 
  get(data, "invoice.colors.primary", default: "#1e293b") 
})

// =============================================================================
// HEADER
// =============================================================================

#grid(
  columns: (1fr, auto),
  column-gutter: 2em,
  [
    #if has-brand {
      brand-logo-image(data, width: 4cm)
    }
    #v(1em)
    #text(weight: "bold")[
      #get(data, "seller.company", default: "Your Company")
    ]
    #get(data, "seller.address", default: "123 Business Street") \
    #get(data, "seller.postal", default: "10001") #get(data, "seller.city", default: "New York") \
    #get(data, "seller.email", default: "billing@company.com") \
    #get(data, "seller.phone", default: "+1 (555) 123-4567")
  ],
  align(right)[
    #text(size: 24pt, weight: "bold", fill: primary-color)[
      #get(data, "labels.invoice", default: "INVOICE")
    ]
    #v(0.5em)
    #text(size: 11pt)[
      #get(data, "labels.number", default: "Number"): #get(data, "invoice.number", default: "INV-0001") \
      #get(data, "labels.date", default: "Date"): #get(data, "invoice.date", default: today()) \
      #get(data, "labels.due", default: "Due Date"): #get(data, "invoice.due_date", default: "")
    ]
  ]
)

#v(2em)

// =============================================================================
// BILL TO
// =============================================================================

#grid(
  columns: (1fr, 1fr),
  column-gutter: 2em,
  [
    #text(weight: "bold", fill: primary-color)[
      #get(data, "labels.bill_to", default: "Bill To")
    ]
    #v(0.5em)
    #get(data, "buyer.name", default: "Customer Company") \
    #get(data, "buyer.contact", default: "Attn: John Doe") \
    #get(data, "buyer.address", default: "456 Client Street") \
    #get(data, "buyer.postal", default: "20002") #get(data, "buyer.city", default: "Los Angeles") \
    #get(data, "buyer.email", default: "accounts@customer.com")
  ],
  [
    #text(weight: "bold", fill: primary-color)[
      #get(data, "labels.payment", default: "Payment Details")
    ]
    #v(0.5em)
    #get(data, "labels.bank", default: "Bank"): #get(data, "payment.bank", default: "Example Bank") \
    #get(data, "labels.iban", default: "IBAN"): #get(data, "payment.iban", default: "XX00 0000 0000 0000") \
    #get(data, "labels.bic", default: "BIC"): #get(data, "payment.bic", default: "EXAMPLEX") \
    #get(data, "labels.reference", default: "Ref"): #get(data, "invoice.number", default: "INV-0001")
  ]
)

#v(2em)

// =============================================================================
// LINE ITEMS
// =============================================================================

#let items = get(data, "items", default: ())
#let currency = get(data, "invoice.currency", default: "USD")

#table(
  columns: (auto, 1fr, auto, auto, auto),
  stroke: (x: none, y: 0.5pt + luma(200)),
  fill: (_, y) => if y == 0 { primary-color } else { none },
  inset: 8pt,
  
  // Header
  align(center)[#text(fill: white)[#get(data, "labels.item", default: "#")]],
  align(left)[#text(fill: white)[#get(data, "labels.description", default: "Description")]],
  align(center)[#text(fill: white)[#get(data, "labels.qty", default: "Qty")]],
  align(right)[#text(fill: white)[#get(data, "labels.price", default: "Price")]],
  align(right)[#text(fill: white)[#get(data, "labels.amount", default: "Amount")]],
  
  // Items
  ..items.enumerate().map(((i, item)) => {
    let qty = item.at("quantity", default: 1)
    let price = item.at("price", default: "0.00")
    let amount = item.at("amount", default: price)
    (
      align(center)[str(i + 1)],
      align(left)[item.at("description", default: "Service")],
      align(center)[str(qty)],
      align(right)[format-currency(price, currency)],
      align(right)[format-currency(amount, currency)]
    )
  }).flatten()
)

#v(1em)

// =============================================================================
// TOTALS
// =============================================================================

#let subtotal = get(data, "invoice.subtotal", default: "0.00")
#let tax-rate = get(data, "invoice.tax_rate", default: "0")
#let tax-amount = get(data, "invoice.tax_amount", default: "0.00")
#let total = get(data, "invoice.total", default: subtotal)

#align(right)[
  #table(
    columns: (auto, auto),
    stroke: none,
    inset: (x: 12pt, y: 4pt),
    align: (right, right),
    
    [#get(data, "labels.subtotal", default: "Subtotal"):], 
    format-currency(subtotal, currency),
    
    [#get(data, "labels.tax", default: "Tax") (#tax-rate%):], 
    format-currency(tax-amount, currency),
    
    table.hline(stroke: 0.5pt + luma(150)),
    
    [#text(weight: "bold", fill: primary-color)[#get(data, "labels.total", default: "Total"):]],
    [#text(weight: "bold", fill: primary-color)[format-currency(total, currency)]],
  )
]

#v(2em)

// =============================================================================
// TERMS & NOTES
// =============================================================================

#line(length: 100%, stroke: 0.5pt + luma(200))
#v(1em)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 2em,
  [
    #text(weight: "bold", fill: primary-color)[
      #get(data, "labels.terms", default: "Terms & Conditions")
    ]
    #v(0.5em)
    #get(data, "invoice.terms", default: "Payment due within 30 days.")
  ],
  [
    #text(weight: "bold", fill: primary-color)[
      #get(data, "labels.notes", default: "Notes")
    ]
    #v(0.5em)
    #get(data, "invoice.notes", default: "Thank you for your business!")
  ]
)

#v(2em)

// =============================================================================
// FOOTER
// =============================================================================

#align(center)[
  #text(size: 9pt, fill: luma(120))[
    #get(data, "seller.company", default: "Your Company") | 
    #get(data, "seller.vat", default: "VAT: XX000000000") |
    #get(data, "seller.registration", default: "Reg: 00000000")
  ]
]
