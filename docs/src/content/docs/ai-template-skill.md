---
title: AI Template Skill
description: Use the Rendrr AI skill to generate Word templates with Claude on claude.ai or Claude Desktop.
---

# AI Template Skill

Generate production-ready `.docx` Handlebars templates by chatting with Claude. Just describe the document you need — an invoice, contract, report, or any other document type — and the AI skill produces a complete Word template with Handlebars placeholders, along with a sample JSON payload you can use to test it.

> **Warning:** This skill is designed for **claude.ai** and **Claude Desktop** only. It is not recommended for Claude Code, as it relies on features (file generation, the `/mnt/skills` filesystem) that are specific to the claude.ai and Claude Desktop environments.

## What the skill does

When you describe a document, the skill will:

1. Design a schema of variables, loops, conditionals, and image slots
2. Generate a `.docx` template using docx-js with Handlebars placeholders
3. Produce a realistic sample JSON payload that exercises all template features
4. Present both files along with a variable reference table

## Setting up the skill

### Step 1: Create the skill directory

In your Claude project, create a skill at the following path:

```
.claude/skills/rendrr-template/SKILL.md
```

If your templates use Handlebars syntax, also add the reference file:

```
.claude/skills/rendrr-template/references/handlebars-syntax.md
```

### Step 2: Copy the SKILL.md content

[**Download `SKILL.md`**](/rendrr/doc-examples/skills/rendrr-template/SKILL.md) or copy the content below into `.claude/skills/rendrr-template/SKILL.md`:

````markdown
---
name: rendrr-template
description: >
  Generate .docx Handlebars templates for the Rendrr Word interpolation API. Use this skill
  whenever the user wants to create a Word document template with dynamic placeholders
  for data-driven document generation. Trigger when the user mentions "template", "Rendrr",
  "Word template", "Handlebars", or describes a document type (invoice, purchase order,
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

```js
new TextRun("{{invoice_number}}")
new TextRun("{{#if discount}}")
new TextRun("{{/if}}")
new TextRun("{{image company_logo width=200}}")
```

### Table loops

Table-based loops require the `{{#each}}` and `{{/each}}` tags in **their own dedicated single-cell rows**. Rendrr strips those rows and repeats the data row(s) between them:

```js
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

- **Header**: Company logo, company name, document title
- **Meta block**: A 2-column table with document metadata (number, date, parties) — use light shading on label cells
- **Body sections**: Use `HeadingLevel.HEADING_1` for major sections, `HEADING_2` for subsections
- **Line item tables**: Full-width table with column widths summing to 9360 DXA (US Letter, 1" margins)
- **Totals block**: Right-aligned table for subtotal / tax / total rows
- **Footer**: Document identifier + page reference
- **Page size**: Always US Letter (12240 × 15840 DXA), 1-inch margins
- **Font**: Arial throughout

---

## Step 3: Sample JSON Payload

After generating the .docx, produce a sample JSON file:

- Realistic values for all scalar variables
- **At least 3 items** in every array
- All conditional flags set to `true` so every optional section is visible
- Image fields: a placeholder URL
- Dates formatted as human-readable strings

---

## Step 4: Present to User

1. Save the .docx and JSON files
2. Include a **Variable Reference** table in your response:

| Variable | Type | Description |
|----------|------|-------------|
| `{{invoice_number}}` | String | Unique invoice identifier |
| `{{line_items}}` | Array | Line items loop |
| `{{company_logo}}` | Image URL | Company logo |
| `{{#if notes}}` | Boolean | Show notes section when truthy |

---

## Quality Checklist

Before presenting files, verify:

- Every `{{#each}}` has a matching `{{/each}}`
- Every `{{#if}}` has a matching `{{/if}}`
- Table loops: `{{#each}}` and `{{/each}}` are in their own single-cell rows
- Every variable in the .docx appears in the sample JSON
- Image fields use `{{image variable_name}}` syntax
- Sample JSON has at least 3 items in each array
- Variable reference table is complete
````

### Step 3: Copy the Handlebars syntax reference

[**Download `handlebars-syntax.md`**](/rendrr/doc-examples/skills/rendrr-template/references/handlebars-syntax.md) or copy the content below into `.claude/skills/rendrr-template/references/handlebars-syntax.md`:

````markdown
# Rendrr Template Syntax Guide

Rendrr uses Handlebars syntax inside DOCX templates. You write your document in Microsoft Word,
insert Handlebars expressions where you want dynamic content, and Rendrr replaces them with
your data at render time.

## Variables

Insert a variable using double curly braces:

{{variable_name}}

Access nested properties using dot notation:

{{user.address.city}}

If a variable is not present in the JSON data, it renders as an empty string.

## Conditionals

{{#if variable}}
  Content shown when variable is truthy.
{{/if}}

{{#if discount}}
  You saved {{discount}}%!
{{else}}
  No discount applied.
{{/if}}

{{#unless paid}}
  PAYMENT OUTSTANDING
{{/unless}}

## Loops

{{#each employees}}
  Name: {{name}}, Role: {{role}}
{{/each}}

### Loop context variables

| Variable      | Description                                      |
|---------------|--------------------------------------------------|
| `{{this}}`    | The current item in the array                    |
| `{{@index}}`  | Zero-based index of the current item             |
| `{{@first}}`  | true if this is the first item                   |
| `{{@last}}`   | true if this is the last item                    |
| `{{@key}}`    | The property name when iterating over an object  |

### Parent context

Use `../` to reference data from the parent scope:

{{#each line_items}}
  {{description}} — Invoice #{{../invoice_number}}
{{/each}}

## Tables

Place `{{#each}}` and `{{/each}}` in their own table rows.
The loop tag rows are removed; the data row is repeated.

## Images

{{image url_variable}}
{{image photo_url width=400}}

Supported formats: PNG, JPEG. Accepts URLs or base64 data URIs.

## Chunk helper

Split an array into groups:

{{#chunk array_name 3}}
  {{#each this}}
    {{this}}
  {{/each}}
{{/chunk}}

## Quick reference

{{variable}}                          Simple variable
{{object.nested.path}}                Nested access
{{#if condition}}...{{/if}}           Conditional
{{#if x}}...{{else}}...{{/if}}        Conditional with fallback
{{#unless condition}}...{{/unless}}   Inverse conditional
{{#each array}}...{{/each}}           Loop
{{this}}                              Current item in loop
{{@index}}                            Loop index (0-based)
{{../parent_var}}                     Parent scope in loop
{{image url}}                         Insert image
{{image url width=400}}               Insert image with max width
{{#chunk array 3}}...{{/chunk}}       Split array into groups
````

### Step 4: Use the skill

Once the files are in place, start a new conversation on **claude.ai** or **Claude Desktop** and ask Claude to generate a template. For example:

- *"Create an invoice template with line items, tax, and a company logo"*
- *"Build a project proposal template with phases, team members, and pricing"*
- *"Make a business letter template with sender/recipient info and enclosures"*

Claude will automatically use the skill to generate a `.docx` template and sample JSON payload.

## Example prompt

> Create a professional invoice template for a consulting company. It should include a company logo, client details, line items with quantity and unit price, subtotal, tax, discount, and total. Add a notes section at the bottom.

Claude will produce:
1. A `.docx` template file with all Handlebars placeholders
2. A sample JSON file with realistic test data
3. A variable reference table documenting every placeholder

You can then upload the `.docx` template to Rendrr via the [Upload a template](./api-reference#uploadTemplate) endpoint and render it using the sample JSON via the [Render a document](./api-reference#renderDocument) endpoint.
