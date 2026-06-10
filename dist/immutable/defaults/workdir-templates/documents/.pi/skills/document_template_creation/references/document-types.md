# Document Type Reference

Comprehensive reference for creating different types of document templates.

## Invoice

**Purpose**: Payment request for goods or services  
**Key Sections**:
- Seller information (company, address, contact)
- Buyer information
- Invoice details (number, date, due date)
- Line items (description, quantity, price, amount)
- Totals (subtotal, tax, total)
- Payment information
- Terms and notes

**Data Structure**:
```toml
[seller]
company = "Company Name"
address = "Street"
postal = "12345"
city = "City"

[buyer]
name = "Customer Name"

[invoice]
number = "INV-0001"
date = "2025-01-15"
due_date = "2025-02-15"
currency = "EUR"
subtotal = "1000.00"
tax_rate = "19"
tax_amount = "190.00"
total = "1190.00"

[[items]]
description = "Service"
quantity = 10
price = "100.00"
amount = "1000.00"
```

**Special Considerations**:
- Currency formatting (symbols, placement)
- Tax calculations
- Line item totaling
- Payment terms clarity
- Late payment penalties

---

## Formal Letter

**Purpose**: Business correspondence  
**Key Sections**:
- Sender (return address)
- Recipient (address block)
- Date and reference
- Subject line
- Salutation
- Body paragraphs
- Closing and signature

**Data Structure**:
```toml
[sender]
company = "Sender Corp"
name = "John Doe"
street = "Main St 1"
postal = "12345"
city = "City"

[recipient]
company = "Recipient Inc"
contact = "Ms. Smith"
street = "Oak Ave 2"

[letter]
date = "January 15, 2025"
subject = "Quote Request"
salutation = "Dear Ms. Smith,"
body = "Main text..."
closing = "Sincerely"
```

**Special Considerations**:
- Follow local standards (DIN 5008 for German)
- Proper address formatting
- Professional tone
- Clear subject line
- Signature block with title

---

## Report

**Purpose**: Technical or business documentation  
**Key Sections**:
- Cover page with metadata
- Executive summary
- Table of contents
- Multiple chapters/sections
- Subsections
- Tables and figures
- Appendix

**Data Structure**:
```toml
[report]
title = "Report Title"
author = "Author Name"
date = "January 2025"
version = "1.0"
summary = "Executive summary..."

[[sections]]
title = "Introduction"
content = "Section content..."

[[sections.subsections]]
title = "Background"
content = "Subsection content..."
```

**Special Considerations**:
- Page numbering
- Heading hierarchy
- Professional typography
- Tables of contents auto-generation
- Confidentiality markings

---

## Contract / Agreement

**Purpose**: Legal agreements between parties  
**Key Sections**:
- Parties identification
- Recitals/background
- Terms and conditions
- Payment terms
- Termination clauses
- Signature blocks

**Data Structure**:
```toml
[parties]
party_a = "Company A"
party_b = "Company B"

[contract]
title = "Service Agreement"
effective_date = "2025-01-01"
term = "12 months"

[[clauses]]
number = "1"
title = "Services"
text = "Provider shall deliver..."

[[signatures]]
party = "Company A"
name = "Authorized Signer"
title = "CEO"
```

**Special Considerations**:
- Legal review requirements
- Clear definitions
- Numbered clauses
- Signature requirements
- Witness/jurat lines
- Version control

---

## Certificate / Award

**Purpose**: Recognition or credential verification  
**Key Sections**:
- Organization header
- Certificate title
- Recipient name
- Achievement description
- Date and location
- Signatures
- Verification info

**Data Structure**:
```toml
[issuer]
organization = "Institute Name"

[recipient]
name = "Recipient Name"
title = "Dr."

[certificate]
type = "Certificate"
subtype = "of Completion"
date = "January 15, 2025"
id = "CERT-0001"

[achievement]
title = "Course Title"
description = "Description..."

[[signatures]]
name = "Director"
title = "Program Director"
```

**Special Considerations**:
- Landscape format often preferred
- Decorative borders
- Gold/official colors
- Verification URLs
- Anti-fraud features

---

## Meeting Minutes / Protocol

**Purpose**: Record of meeting proceedings  
**Key Sections**:
- Meeting metadata (title, date, location)
- Attendees (present/absent)
- Agenda items
- Discussion summary
- Decisions made
- Action items

**Data Structure**:
```toml
[meeting]
title = "Project Review"
date = "2025-01-15"
time = "10:00-11:30"
location = "Conference Room A"

[[attendees]]
name = "John Doe"
present = true

[[items]]
title = "Status Update"
discussion = "Team reported..."
decisions = ["Decision 1"]

[[items.actions]]
task = "Follow up"
assigned = "Jane Smith"
due = "2025-01-22"
```

**Special Considerations**:
- Clear action item tracking
- Decision recording
- Attendance tracking
- Distribution list

---

## Agenda

**Purpose**: Meeting schedule and topics  
**Key Sections**:
- Event title and details
- Time schedule
- Topic list with presenters
- Breaks
- Materials needed

**Data Structure**:
```toml
[event]
title = "Workshop Title"
date = "January 15, 2025"
time = "09:00-17:00"
location = "Meeting Room"

[[items]]
time = "09:00"
title = "Welcome"
presenter = "Moderator"
duration = "15 min"
```

**Special Considerations**:
- Time management
- Buffer time
- Material preparation
- Pre-reading distribution

---

## Resume / CV

**Purpose**: Personal professional profile  
**Key Sections**:
- Personal info
- Summary/Objective
- Experience
- Education
- Skills
- Certifications

**Data Structure**:
```toml
[profile]
name = "John Doe"
email = "john@example.com"
phone = "+1 555-1234"
summary = "Experienced professional..."

[[experience]]
company = "Corp"
position = "Manager"
period = "2020-2025"
description = "Responsibilities..."

[[education]]
institution = "University"
degree = "Bachelor"
year = "2019"
```

**Special Considerations**:
- ATS compatibility
- Clear hierarchy
- Action verbs
- Quantifiable achievements
- Consistent formatting

---

## Specification

**Purpose**: Technical requirements document  
**Key Sections**:
- Overview
- Requirements (functional/non-functional)
- Constraints
- Acceptance criteria
- Version history

**Data Structure**:
```toml
[spec]
title = "System Specification"
version = "1.0"

[[requirements]]
id = "REQ-001"
type = "functional"
priority = "high"
description = "System shall..."
```

**Special Considerations**:
- Traceability
- Testability
- Atomic requirements
- Priority assignment
- Change tracking
