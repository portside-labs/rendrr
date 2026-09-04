---
title: Template Syntax
description: Handlebars syntax reference for Rendrr templates.
---

# Template Syntax Guide

Rendrr uses [Handlebars](https://handlebarsjs.com/) syntax inside your templates. Author your template in your editor of choice (Microsoft Word, or any other producer of `.docx` files), insert Handlebars expressions where you want dynamic content, and Rendrr replaces them with your data at render time.

The syntax below is the same across all supported formats — only sections explicitly marked as format-specific (such as headers and footers) apply to a single format.

> **Before you can render documents, you need to upload a template first.** See the [Upload a template](./api-reference#uploadTemplate) endpoint in the API reference to get your `template_id`, then use it in the examples below.

## How it works

1. Upload a template (`.docx`) containing Handlebars expressions
2. Send a render request with JSON data
3. Rendrr replaces all expressions with your data and returns the rendered document

Every example below includes a ready-to-run cURL request against `http://localhost:3000` (the default when running Rendrr locally). Replace `YOUR_TEMPLATE_ID` with the ID of your uploaded template, swap the base URL for your own deployment, and paste into a terminal to render the document.

> If you've enabled OAuth on your Rendrr instance, add `-H "Authorization: Bearer <jwt>"` to every request below. With auth disabled (the default), no header is required.

## Variables

Insert a variable using double curly braces. In your document, type the expression wherever you want a dynamic value to appear:

**In your document:**

![Template with a customer_name variable](/rendrr/doc-examples/template-syntax/before/01-variables.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "customer_name": "Jane Smith"
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document with customer name filled in](/rendrr/doc-examples/template-syntax/after/01-variables.png)

### Nested object paths

Access nested properties using dot notation.

**In your document:**

![Template using nested user.address.city paths](/rendrr/doc-examples/template-syntax/before/02-nested.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "user": {
        "name": "Jane Smith",
        "address": {
          "street": "123 Main St",
          "city": "Portland",
          "state": "OR"
        }
      }
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing Jane Smith in Portland, OR](/rendrr/doc-examples/template-syntax/after/02-nested.png)

### Array indexing

Access a specific element of an array by its zero-based index using bracket notation. The square brackets are required — `items.0.name` is not a valid path, but `items.[0].name` is.

**In your document:**

```
First customer: {{customers.[0].name}}
Second customer: {{customers.[1].name}}
Their city: {{customers.[0].address.city}}
```

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "customers": [
        { "name": "Jane Smith", "address": { "city": "Portland" } },
        { "name": "Bob Lee",    "address": { "city": "Austin" } }
      ]
    },
    "output_format": "docx"
  }'
```

You can chain indices and property names freely — `orders.[2].line_items.[0].sku` reaches into the first line item of the third order. Out-of-range indices render as an empty string (the same as any missing variable).

> **Authoring tip:** Brackets are a spot where Word frequently breaks a placeholder across runs (often because the editor inserts hidden spell-check or formatting markers around `[` and `]`). Rendrr normalizes these splits automatically, but if you see a bracketed expression coming through unrendered, retype the whole expression in one go inside a single text run.

### Indexing inside loops

Inside an `{{#each}}` block, three special variables let you check the current position:

| Variable      | Description                                      |
|---------------|--------------------------------------------------|
| `{{@index}}`  | Zero-based index of the current item (0, 1, 2…)  |
| `{{@first}}`  | `true` if this is the first item                 |
| `{{@last}}`   | `true` if this is the last item                  |

These are most useful with `{{#if}}` for separator characters, special formatting on the first/last row of a table, numbered lists, and similar patterns:

```
{{#each rows}}
  {{#if @first}}Top of list:{{/if}}
  {{@index}}. {{this.name}}{{#unless @last}},{{/unless}}
{{/each}}
```

To reference the parent scope from inside a loop, use `../` (see [Accessing parent context](#accessing-parent-context) below for a full example).

### Missing variables

If a variable is not present in the JSON data, it renders as an empty string. Your document will not show an error — the placeholder is simply removed.

## Conditionals

Show or hide sections of your document based on your data.

### `{{#if}}`

Wrap content between `{{#if variable}}` and `{{/if}}` and it will only appear when the value is truthy.

**In your document:**

![Template with an if block around premium membership text](/rendrr/doc-examples/template-syntax/before/03-if.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "is_premium": true
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the premium message](/rendrr/doc-examples/template-syntax/after/03-if.png)

If `is_premium` is `false`, `null`, `0`, an empty string, or missing entirely, the content between `{{#if}}` and `{{/if}}` is omitted.

### `{{else}}`

Provide fallback content when the condition is falsy:

**In your document:**

![Template with an if/else block for a discount message](/rendrr/doc-examples/template-syntax/before/04-if-else.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "discount": 15
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the discount branch: You saved 15%!](/rendrr/doc-examples/template-syntax/after/04-if-else.png)

### `{{unless}}`

The inverse of `{{#if}}` — renders content only when the value is falsy.

**In your document:**

![Template with an unless block around a PAYMENT OUTSTANDING notice](/rendrr/doc-examples/template-syntax/before/05-unless.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "paid": false
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the PAYMENT OUTSTANDING notice](/rendrr/doc-examples/template-syntax/after/05-unless.png)

## Loops

Iterate over arrays to repeat sections of your document.

### `{{#each}}`

Place `{{#each array}}` and `{{/each}}` around the content you want repeated. Inside the loop, `{{this}}` refers to the current item.

### Simple arrays

**In your document:**

![Template with each loop printing items separated by commas](/rendrr/doc-examples/template-syntax/before/06-each-simple.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "items": ["Widget", "Gadget", "Gizmo"]
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing Items: Widget, Gadget, Gizmo,](/rendrr/doc-examples/template-syntax/after/06-each-simple.png)

### Arrays of objects

Access properties of each object directly inside the loop:

**In your document:**

![Template iterating over employees and printing name and role](/rendrr/doc-examples/template-syntax/before/07-each-objects.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "employees": [
        { "name": "Alice", "role": "Engineer" },
        { "name": "Bob", "role": "Designer" }
      ]
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document listing Alice and Bob with their roles](/rendrr/doc-examples/template-syntax/after/07-each-objects.png)

### Loop context variables

Inside an `{{#each}}` block, Handlebars provides special variables:

| Variable      | Description                                      |
|---------------|--------------------------------------------------|
| `{{this}}`    | The current item in the array                    |
| `{{@index}}`  | Zero-based index of the current item (0, 1, 2…)  |
| `{{@first}}`  | `true` if this is the first item                 |
| `{{@last}}`   | `true` if this is the last item                  |
| `{{@key}}`    | The property name when iterating over an object  |

### Accessing parent context

Use `../` to reference data from the parent scope.

**In your document:**

![Template using ../invoice_number to reference the parent scope inside an each loop](/rendrr/doc-examples/template-syntax/before/08-parent-context.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "invoice_number": "INV-2026-0042",
      "line_items": [
        { "description": "Consulting" },
        { "description": "Development" },
        { "description": "Support" }
      ]
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing each line item with the parent invoice number](/rendrr/doc-examples/template-syntax/after/08-parent-context.png)

## Tables

To generate dynamic table rows, place `{{#each}}` and `{{/each}}` in their own single-cell rows immediately above and below the row you want repeated. Rendrr strips the loop-tag rows and duplicates the row between them for each item in the array.

**In your document:**

![Template with a table where each/end each tags are on their own rows surrounding the data row](/rendrr/doc-examples/template-syntax/before/09-table.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "rows": [
        { "name": "Consulting", "amount": "$5,000" },
        { "name": "Development", "amount": "$12,000" },
        { "name": "Support", "amount": "$3,000" }
      ]
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document with a table populated with three service rows](/rendrr/doc-examples/template-syntax/after/09-table.png)

## Images

Insert dynamic images from URLs or base64-encoded data using the `image` helper.

### Basic usage

Type `{{image variable_name}}` where you want the image to appear. At render time Rendrr downloads (or decodes) the image and embeds it in the document.

**In your document:**

![Template with an image helper referencing company_logo](/rendrr/doc-examples/template-syntax/before/10-image.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "company_logo": "https://example.com/logo.png"
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the Acme Solutions LLC logo inline](/rendrr/doc-examples/template-syntax/after/10-image.png)

### With a width constraint

Limit the maximum width (in pixels). The image maintains its aspect ratio.

**In your document:**

![Template with an image helper that includes a width=400 argument](/rendrr/doc-examples/template-syntax/before/11-image-width.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "photo_url": "https://example.com/photo.png"
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the image scaled to a 400-pixel max width](/rendrr/doc-examples/template-syntax/after/11-image-width.png)

### Base64-encoded images

Instead of a URL, you can pass the image data directly as a base64 data URI:

```json
{
  "signature": "data:image/png;base64,iVBORw0KGgo..."
}
```

### Supported formats

- PNG
- JPEG

## Chunk helper

Split an array into groups of a fixed size. Useful for grid layouts — for example, rendering 3 items per row in a table.

**In your document:**

![Template using the chunk helper to group products into pairs](/rendrr/doc-examples/template-syntax/before/12-chunk.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "products": [
        { "name": "A" },
        { "name": "B" },
        { "name": "C" },
        { "name": "D" },
        { "name": "E" }
      ]
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the five products grouped two-per-line](/rendrr/doc-examples/template-syntax/after/12-chunk.png)

## Headers and footers (DOCX only)

Template expressions work in headers and footers too. Rendrr processes all header and footer XML parts with the same data context as the document body.

**In your document:**

![Template with expressions in the page header and footer](/rendrr/doc-examples/template-syntax/before/13-header-footer.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "company_name": "Acme Solutions LLC",
      "recipient": "Jane Smith",
      "date": "April 15, 2026"
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document with header and footer expressions filled in](/rendrr/doc-examples/template-syntax/after/13-header-footer.png)

## Special characters and encoding

Handlebars HTML-escapes values by default. In practice this means:

| Character | Rendered as |
|-----------|-------------|
| `&`       | `&amp;`     |
| `<`       | `&lt;`      |
| `>`       | `&gt;`      |
| `"`       | `&quot;`    |

Since DOCX is an XML-based format under the hood, this escaping is generally correct and keeps your document well-formed.

To insert unescaped content, use triple braces.

**In your document:**

![Template comparing a double-brace escaped variable with a triple-brace unescaped variable](/rendrr/doc-examples/template-syntax/before/14-raw.png)

**Try it with cURL:**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "YOUR_TEMPLATE_ID",
    "data": {
      "company_name": "Smith & Jones, LLC",
      "raw_html": "Smith & Jones, LLC"
    },
    "output_format": "docx"
  }'
```

**Rendered output:**

![Rendered document showing the escaped ampersand as &amp; and the unescaped version as &](/rendrr/doc-examples/template-syntax/after/14-raw.png)

> **Warning:** Only use triple braces when you are certain the value contains valid XML. Malformed content will corrupt the document.

Unicode characters (accented letters, emoji, CJK, etc.) are fully supported and require no special handling.

## Limits

Rendrr enforces the following limits to ensure safe and predictable rendering:

| Limit                | Value   |
|----------------------|---------|
| JSON nesting depth   | 100 levels |
| Array size           | 10,000 items |
| JSON payload size    | 10 MB   |

Exceeding these limits returns an error before rendering begins.

## Template validation

Rendrr validates your template when you upload it. Common errors caught at upload time:

| Error                   | Example                         | Problem                                             |
|-------------------------|---------------------------------|-----------------------------------------------------|
| `UnclosedBlock`         | `{{#each items}}` with no close | Missing `{{/each}}`                                 |
| `UnmatchedClosingTag`   | `{{/each}}` with no open        | Closing tag without a matching opening tag           |
| `MismatchedClosingTag`  | `{{#each items}}...{{/if}}`     | Opening and closing tags don't match                 |
| `MissingParameter`      | `{{#chunk items}}`              | `chunk` requires a size parameter (e.g., `{{#chunk items 3}}`) |

### XML run splitting

Word sometimes splits a single expression like `{{name}}` across multiple internal XML runs — typically when the editor inserts hidden formatting, spell-check markers, or bookmarks mid-placeholder. Rendrr automatically normalizes these splits before rendering, so you don't need to worry about it. If you ever see a placeholder coming through unrendered, retype the entire expression in one go inside a single text run.

## Quick reference

```
{{variable}}                          Simple variable
{{object.nested.path}}                Nested access
{{array.[0]}}                         Array element by index
{{array.[2].nested.field}}            Indexed access + nested path
{{#if condition}}...{{/if}}           Conditional
{{#if x}}...{{else}}...{{/if}}        Conditional with fallback
{{#unless condition}}...{{/unless}}   Inverse conditional
{{#each array}}...{{/each}}           Loop
{{this}}                              Current item in loop
{{@index}}                            Loop index (0-based)
{{@first}} / {{@last}}                True on first/last loop item
{{@key}}                              Property name when iterating an object
{{../parent_var}}                     Parent scope in loop
{{image url}}                         Insert image
{{image url width=400}}               Insert image with max width
{{#chunk array 3}}...{{/chunk}}       Split array into groups
```
