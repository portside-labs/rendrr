---
name: rendrr-template
description: >
  Generate .docx Handlebars templates for the Rendrr Word interpolation API. Use this skill
  whenever the user wants to create a Word document template with dynamic placeholders
  for data-driven document generation. Trigger when the user mentions "template", "Rendrr",
  "Word template", "Handlebars", or derendrrs a document type (invoice, purchase order,
  business letter, audit report, project plan, contract, proposal, etc.) that should be
  populated with dynamic data. Also trigger when the user asks to "generate a template",
  "make a template", or "build a document template" even if they don't name Rendrr explicitly.
---

# Rendrr Template Generator

Generates production-ready `.docx` Handlebars templates for the Rendrr document rendering
API, along with a companion sample JSON payload.

## Workflow

1. **Understand the request** — parse the natural language description to determine document type, required fields, and any custom requirements
2. **Design the schema** — plan all variables, loops, conditionals, and image slots before writing code
3. **Read the docx skill** — always read `/mnt/skills/public/docx/SKILL.md` before generating files
4. **Generate the .docx** — build the template using docx-js with Handlebars placeholders as plain text
5. **Generate the JSON payload** — produce a realistic sample JSON that exercises all template features
6. **Present both files** to the user with a variable reference table

---

## Step 1: Schema Design

Before writing any code, plan the template on paper. For each document type, identify:

- **Scalar variables** — simple `{{variable}}` placeholders (names, dates, amounts, addresses)
- **Conditional blocks** — `{{#if condition}}...{{/if}}` for optional sections
- **Loop blocks** — `{{#each array}}...{{/each}}` for line items, employees, tasks, etc.
- **Images** — `{{image variable_name}}` for logos, signatures, photos
- **Parent scope references** — `{{../parent_var}}` when needed inside loops

### Common document schemas (reference — adapt freely)

**Invoice:**
- Scalars: `invoice_number`, `invoice_date`, `due_date`, `company_name`, `company_address`, `client_name`, `client_address`, `subtotal`, `tax_rate`, `tax_amount`, `total`, `notes`
- Loop: `line_items[]` → `{ description, quantity, unit_price, amount }`
- Image: `company_logo`
- Conditionals: `{{#if notes}}`, `{{#if discount}}`

**Purchase Order:**
- Scalars: `po_number`, `po_date`, `vendor_name`, `vendor_address`, `ship_to_name`, `ship_to_address`, `payment_terms`, `delivery_date`, `subtotal`, `total`
- Loop: `items[]` → `{ line_number, description, quantity, unit_price, total }`
- Image: `company_logo`
- Conditionals: `{{#if special_instructions}}`

**Business Letter:**
- Scalars: `sender_name`, `sender_title`, `company_name`, `company_address`, `date`, `recipient_name`, `recipient_title`, `recipient_company`, `recipient_address`, `subject`, `body`, `closing`
- Image: `company_logo`, `signature_image`
- Conditionals: `{{#if subject}}`, `{{#if enclosures}}`
- Loop: `enclosures[]` → `{ description }`

**Audit Report:**
- Scalars: `report_title`, `audit_date`, `auditor_name`, `entity_name`, `period_covered`, `executive_summary`
- Loops: `findings[]` → `{ id, title, severity, description, recommendation }`, `appendices[]` → `{ title, content }`
- Conditionals: `{{#if critical_findings}}`, `{{#unless fully_satisfactory}}`

**Project Plan:**
- Scalars: `project_name`, `project_manager`, `start_date`, `end_date`, `budget`, `sponsor`, `objective`, `scope`
- Loops: `milestones[]` → `{ name, due_date, owner, status }`, `risks[]` → `{ description, likelihood, impact, mitigation }`, `team[]` → `{ name, role }`
- Conditionals: `{{#if risks}}`, `{{#if budget_notes}}`

For document types not listed, derive an appropriate schema from first principles based on real-world usage of that document type.

---

## Step 2: Generate the .docx

Read `/mnt/skills/public/docx/SKILL.md` before writing code. Then use docx-js to produce the file.

### Critical: How Handlebars expressions work in docx-js

Handlebars expressions are **plain text strings** inside `TextRun`. Rendrr replaces them at render time — the Word file just needs to contain the literal characters `{{...}}`:

```javascript
new TextRun("{{invoice_number}}")
new TextRun("{{#if discount}}")
new TextRun("{{/if}}")
new TextRun("{{image company_logo width=200}}")
```

### Table loops

Table-based loops require the `{{#each}}` and `{{/each}}` tags in **their own dedicated single-cell rows**. Rendrr strips those rows and repeats the data row(s) between them:

```javascript
new Table({
  rows: [
    // Header row (static — not inside the loop)
    new TableRow({ children: [
      headerCell("Description"), headerCell("Qty"), headerCell("Amount")
    ]}),
    // Loop open row — Rendrr removes this row
    new TableRow({ children: [
      new TableCell({ children: [new Paragraph({ children: [new TextRun("{{#each line_items}}")] })] })
    ]}),
    // Data row — Rendrr repeats this for each item
    new TableRow({ children: [
      dataCell("{{description}}"), dataCell("{{quantity}}"), dataCell("{{amount}}")
    ]}),
    // Loop close row — Rendrr removes this row
    new TableRow({ children: [
      new TableCell({ children: [new Paragraph({ children: [new TextRun("{{/each}}")] })] })
    ]}),
  ]
})
```

### Document structure conventions

Follow these layout patterns for professional output:

- **Header**: Company logo (`{{image company_logo width=180}}`), company name, document title
- **Meta block**: A 2-column table with document metadata (number, date, parties) — use light shading on label cells
- **Body sections**: Use `HeadingLevel.HEADING_1` for major sections, `HEADING_2` for subsections
- **Line item tables**: Full-width table with column widths summing to 9360 DXA (US Letter, 1" margins)
- **Totals block**: Right-aligned table for subtotal / tax / total rows
- **Footer**: Document identifier + page reference (e.g., `{{document_type}} | {{document_number}} | {{date}}`)
- **Page size**: Always US Letter (12240 × 15840 DXA), 1-inch margins
- **Font**: Arial throughout

### Shading helper pattern

Use this pattern for consistent table styling:

```javascript
const headerCell = (text) => new TableCell({
  shading: { fill: "2E5FA3", type: ShadingType.CLEAR },
  margins: { top: 80, bottom: 80, left: 120, right: 120 },
  children: [new Paragraph({
    alignment: AlignmentType.CENTER,
    children: [new TextRun({ text, bold: true, color: "FFFFFF", font: "Arial", size: 20 })]
  })]
});

const dataCell = (text, align = AlignmentType.LEFT) => new TableCell({
  borders: { ... },
  margins: { top: 80, bottom: 80, left: 120, right: 120 },
  children: [new Paragraph({
    alignment: align,
    children: [new TextRun({ text, font: "Arial", size: 20 })]
  })]
});
```

---

## Step 3: Sample JSON Payload

After generating the .docx, produce a `<template_name>-sample.json` file:

- Realistic values for all scalar variables (not "string1", "value2" — use real-looking data)
- **At least 3 items** in every array
- All conditional flags set to `true` so every optional section is visible
- Image fields: `"https://via.placeholder.com/200x80.png"` as the URL
- Dates formatted as human-readable strings (e.g., `"March 15, 2025"`) since Rendrr passes them through as-is

```json
{
  "invoice_number": "INV-2025-0042",
  "invoice_date": "March 15, 2025",
  "due_date": "April 14, 2025",
  "company_logo": "https://via.placeholder.com/200x80.png",
  "company_name": "Acme Solutions LLC",
  "line_items": [
    { "description": "Consulting Services", "quantity": "10 hrs", "unit_price": "$150.00", "amount": "$1,500.00" },
    { "description": "Software License (Annual)", "quantity": "1", "unit_price": "$500.00", "amount": "$500.00" },
    { "description": "Implementation Support", "quantity": "5 hrs", "unit_price": "$150.00", "amount": "$750.00" }
  ],
  "subtotal": "$2,750.00",
  "tax_rate": "7%",
  "tax_amount": "$192.50",
  "total": "$2,942.50",
  "notes": "Payment due within 30 days. Thank you for your business.",
  "discount": true
}
```

---

## Step 4: Present to User

1. Save the .docx to `/mnt/user-data/outputs/<template_name>.docx`
2. Save the JSON to `/mnt/user-data/outputs/<template_name>-sample.json`
3. Call `present_files` with both files (docx first)
4. In your response, include a **Variable Reference** table:

| Variable | Type | Description |
|----------|------|-------------|
| `{{invoice_number}}` | String | Unique invoice identifier |
| `{{line_items}}` | Array | Line items loop — see fields below |
| `{{line_items[].description}}` | String | Line item description |
| `{{company_logo}}` | Image URL | Company logo (PNG/JPEG URL or base64) |
| `{{#if notes}}` | Boolean | Show notes section when truthy |

---

## Handlebars Syntax Reference

See `/mnt/skills/public/rendrr-template/references/handlebars-syntax.md` for the full Rendrr Handlebars reference. Read it when you need to verify syntax for specific features: images with width constraints, the `{{#chunk}}` helper, `{{@index}}`/`{{@first}}`/`{{@last}}` loop variables, `{{../parent}}` parent scope, or triple-brace unescaped content.

---

## Quality Checklist

Before presenting files, verify:

- [ ] Every `{{#each}}` has a matching `{{/each}}`
- [ ] Every `{{#if}}` has a matching `{{/if}}`
- [ ] Table loops: `{{#each}}` and `{{/each}}` are in their own single-cell rows
- [ ] Every variable in the .docx appears in the sample JSON
- [ ] Image fields use `{{image variable_name}}` syntax (not `{{variable_name}}`)
- [ ] Sample JSON has at least 3 items in each array
- [ ] The .docx validates without errors (`python scripts/office/validate.py`)
- [ ] Variable reference table is complete