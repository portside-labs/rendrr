# Rendrr Template Syntax Guide

Rendrr uses [Handlebars](https://handlebarsjs.com/) syntax inside DOCX templates. You write your document in Microsoft Word (or any editor that produces `.docx` files), insert Handlebars expressions where you want dynamic content, and Rendrr replaces them with your data at render time.

## How it works

1. Upload a `.docx` template containing Handlebars expressions
2. Send a render request with JSON data
3. Rendrr replaces all expressions with your data and returns the rendered document

---

## Variables

Insert a variable using double curly braces:

```
{{variable_name}}
```

**In your Word document:**

> Dear {{customer_name}}, thank you for your order.

**JSON data:**

```json
{
  "customer_name": "Jane Smith"
}
```

**Result:**

> Dear Jane Smith, thank you for your order.

### Nested object paths

Access nested properties using dot notation:

```
{{user.address.city}}
```

**JSON data:**

```json
{
  "user": {
    "name": "Jane Smith",
    "address": {
      "street": "123 Main St",
      "city": "Portland",
      "state": "OR"
    }
  }
}
```

**In your template:**

> {{user.name}} lives in {{user.address.city}}, {{user.address.state}}.

**Result:**

> Jane Smith lives in Portland, OR.

### Missing variables

If a variable is not present in the JSON data, it renders as an empty string. Your document will not show an error — the placeholder is simply removed.

---

## Conditionals

Show or hide sections of your document based on your data.

### `{{#if}}`

```
{{#if variable}}
  This content only appears if variable is truthy.
{{/if}}
```

If `is_premium` is `false`, `null`, `0`, an empty string, or missing entirely, the content between `{{#if}}` and `{{/if}}` is omitted.

### `{{else}}`

Provide fallback content when the condition is falsy:

```
{{#if discount}}
  You saved {{discount}}%!
{{else}}
  No discount applied.
{{/if}}
```

### `{{unless}}`

The inverse of `{{#if}}` — renders content only when the value is falsy:

```
{{#unless paid}}
  PAYMENT OUTSTANDING
{{/unless}}
```

---

## Loops

Iterate over arrays to repeat sections of your document.

### `{{#each}}`

```
{{#each array_name}}
  {{this}}
{{/each}}
```

### Arrays of objects

Access properties of each object directly inside the loop:

```
{{#each employees}}
Name: {{name}}, Role: {{role}}
{{/each}}
```

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

Use `../` to reference data from the parent scope:

```
{{#each line_items}}
  {{description}} — Invoice #{{../invoice_number}}
{{/each}}
```

---

## Tables

To generate dynamic table rows, place `{{#each}}` and `{{/each}}` in their own table rows surrounding the row you want repeated.

**Table structure in Word:**

| Row 1 | `{{#each rows}}`          |  ← this entire row is removed at render time
| Row 2 | `{{name}}` | `{{amount}}` |  ← this row is repeated for each item
| Row 3 | `{{/each}}`               |  ← this entire row is removed at render time

Row 1 and Row 3 (containing the loop tags) are removed in the output. Row 2 is duplicated for each item in the array.

---

## Images

Insert dynamic images from URLs or base64-encoded data.

### Basic usage

```
{{image url_variable}}
```

### With a width constraint

Limit the maximum width (in pixels). The image maintains its aspect ratio:

```
{{image photo_url width=400}}
```

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

---

## Chunk helper

Split an array into groups of a fixed size. Useful for grid layouts — for example, rendering 3 items per row in a table.

```
{{#chunk array_name size}}
  {{#each this}}
    {{this}}
  {{/each}}
{{/chunk}}
```

---

## Headers and footers

Template expressions work in headers and footers too. Rendrr processes all header and footer XML files in the DOCX with the same data context as the document body.

---

## Special characters and encoding

Handlebars HTML-escapes values by default:

| Character | Rendered as |
|-----------|-------------|
| `&`       | `&amp;`     |
| `<`       | `&lt;`      |
| `>`       | `&gt;`      |
| `"`       | `&quot;`    |

To insert unescaped content, use triple braces:

```
{{{raw_html}}}
```

> **Warning:** Only use triple braces when you are certain the value contains valid XML.

---

## Limits

| Limit                | Value      |
|----------------------|------------|
| JSON nesting depth   | 100 levels |
| Array size           | 10,000 items |
| JSON payload size    | 10 MB      |

---

## Template validation errors

| Error                   | Problem                                              |
|-------------------------|------------------------------------------------------|
| `UnclosedBlock`         | Missing `{{/each}}` or `{{/if}}`                     |
| `UnmatchedClosingTag`   | Closing tag without a matching opening tag           |
| `MismatchedClosingTag`  | Opening and closing tags don't match                 |
| `MissingParameter`      | `{{#chunk items}}` missing size (e.g., `{{#chunk items 3}}`) |

---

## Quick reference

```
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
```