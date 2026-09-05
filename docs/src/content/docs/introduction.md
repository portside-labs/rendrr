---
title: Introduction
description: What Rendrr is, what it does, and when to reach for it.
---

# What is Rendrr?

**Rendrr turns a Word document into an API.**

You author a `.docx` in Word exactly as you normally would, and type
`{{customer_name}}` wherever a value should go. You POST that file to Rendrr
once, then POST JSON to it whenever you need a document. You get back a
finished `.docx` or PDF.

It's a single Rust binary that runs as one Docker container on your own
infrastructure. Your documents and data never leave it.

## See it

<div class="before-after">
  <figure>
    <img src="/rendrr/doc-examples/introduction/before-template.png" alt="A Word template with Handlebars placeholders such as customer_name in place of real values" />
    <figcaption><strong>The template.</strong> Ordinary Word. The placeholders are just text you type.</figcaption>
  </figure>
  <figure>
    <img src="/rendrr/doc-examples/introduction/after-rendered.png" alt="The same document rendered, with the placeholders replaced by real values from JSON" />
    <figcaption><strong>The result.</strong> Same layout, fonts, and styling — your data filled in.</figcaption>
  </figure>
</div>

Formatting is preserved because Rendrr edits the document in place. It doesn't
convert your file to HTML or Markdown and rebuild it, so the output opens in
Word looking exactly like what you designed.

## The problem it solves

Generating documents in code usually means one of these, and all of them hurt:

- **Building the document programmatically** — laying out tables and styling
  runs in a library API. Every layout tweak becomes a code change and a deploy.
- **HTML → PDF** — you get a PDF, but never a `.docx` anyone can edit, and
  print fidelity is a fight.
- **A licensed Office server** — works, but it's Windows, expensive, and a
  service to operate.

Rendrr splits the two concerns. **Design lives in the document**, where the
people who care about wording and layout can edit it in Word without touching
code. **Data lives in your application**, as ordinary JSON. Changing how an
invoice looks means editing a `.docx` and re-uploading it.

## How it works

Three calls. There's no SDK to install — it's plain HTTP.

**1. Upload the template once.** You get back a `template_id`.

```bash
curl -X POST http://localhost:3000/v1/templates \
  -F "file=@invoice.docx"
```

**2. Render it with data, as often as you like.**

```bash
curl -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d '{
    "template_id": "01933e5c-8f2a-7890-a1b2-c3d4e5f60001",
    "data": { "customer_name": "Acme Corp", "total": "1,240.00" },
    "output_format": "pdf"
  }'
```

**3. Download the result.**

```bash
curl -OJ http://localhost:3000/v1/renders/{render_id}/download
```

`output_format` is the only difference between getting a `.docx` and a PDF.
The PDF is produced in-process — there's no LibreOffice or headless browser to
run alongside it.

## What you can put in a template

More than variable substitution. The full list is in
[Template syntax](./template-syntax), but in short:

| You want | You write |
| --- | --- |
| A value | `{{customer_name}}` |
| A nested value | `{{customer.address.city}}` |
| A repeating table row | `{{#each line_items}}` … `{{/each}}` |
| Optional content | `{{#if past_due}}` … `{{/if}}` |
| An image from a URL or base64 | `{{image logo_url}}` |
| A grid, N items per row | `{{#chunk photos 3}}` … `{{/chunk}}` |

Loops repeat real Word table rows, so borders and shading carry through.
Placeholders work in headers and footers too.

## What Rendrr is not

Worth knowing before you invest time:

- **Not a document editor or previewer.** It has no UI. You design in Word;
  Rendrr is the render step in your backend.
- **Not a PDF form filler.** It fills Word templates. If you need to populate
  fields in an existing PDF, this is the wrong tool.
- **Not PowerPoint or Excel.** `.docx` only. PPTX was removed before the first
  open-source release because it was incomplete.
- **Not a converter for arbitrary documents.** Input is a `.docx` you control
  and have added placeholders to.

## What running it involves

Rendrr is stateless. It keeps no database, and it needs exactly one thing from
you: **an S3-compatible bucket** for templates and rendered files. AWS S3,
MinIO, Cloudflare R2, Backblaze B2, and GCS all work.

Everything else is optional and off by default:

- **PDF output** is built in — no sidecar service.
- **OAuth 2.0 / OIDC** locks the API behind your existing identity provider,
  with per-endpoint scopes. Set one environment variable to enable it.
- **TLS** can be terminated by Rendrr directly, or by a proxy in front of it.

The [Getting started](./getting-started) guide brings up Rendrr and a
local MinIO bucket with one `docker compose up`, so you can try it before
deciding where anything lives.

## Where to go next

- **[Getting started](./getting-started)** — running and rendering your
  first document, in a few minutes.
- **[Template syntax](./template-syntax)** — every expression and helper,
  with before/after screenshots for each.
- **[API reference](./api-reference)** — the five endpoints, request and
  response shapes, and error formats.
- **[OAuth 2.0](./oauth)** — putting the API behind your IdP.
- **[Deployment](./deployment)** — Docker, Kubernetes, and
  containers-as-a-service patterns.
- **[AI template skill](./ai-template-skill)** — generate a starting
  template by describing the document to Claude.
