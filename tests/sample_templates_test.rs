//! Renders the sample templates shipped in `docs/public/samples/` with their
//! documented payloads and asserts the data actually lands in the output.
//!
//! The rest of the suite builds minimal DOCX fixtures by hand, which exercises
//! the engine but not the shape of a document Word actually produces — split
//! runs, table structures, headers. These are real files, so a templating
//! regression that the synthetic fixtures miss shows up here.
//!
//! Image URLs in the payloads are swapped for an inline base64 data URL, so
//! the `{{image}}` helper still runs (the samples call it in their headers)
//! without the test needing network access or tripping the SSRF guard.

use bytes::Bytes;
use rendrr::docx::DocxTemplateEngine;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

/// A 1x1 transparent PNG as a data URL — enough for the `{{image}}` helper to
/// decode and embed without a network fetch.
const PIXEL_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

/// Pull `word/document.xml` back out of a rendered DOCX so we can assert on
/// the text the user would see.
fn document_xml(docx: &Bytes) -> String {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(docx.to_vec()))
        .expect("rendered output should be a valid ZIP");
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .expect("rendered DOCX should contain word/document.xml")
        .read_to_string(&mut xml)
        .expect("document.xml should be readable UTF-8");
    xml
}

/// Word splits text across runs, so a value can be interrupted by tags. Strip
/// every XML tag, then decode entities — Handlebars escapes `&` to `&amp;`
/// on the way in, so a description like "Implementation & Onboarding" only
/// matches its payload string after decoding.
fn visible_text(xml: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in xml.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // `&amp;` must be decoded last, or `&amp;lt;` would become `<`.
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

async fn render_sample(name: &str, file: &str) -> String {
    let dir = Path::new("docs/public/samples").join(name);
    let template =
        Bytes::from(std::fs::read(dir.join(file)).expect("sample template should exist"));
    let payload: Value = serde_json::from_slice(
        &std::fs::read(dir.join("payload.json")).expect("sample payload should exist"),
    )
    .expect("payload.json should be valid JSON");

    // The sample payloads wrap the render data under a "data" key, matching the
    // request body documented for POST /v1/renders.
    let mut data = payload
        .get("data")
        .cloned()
        .unwrap_or_else(|| payload.clone());

    // Swap remote image URLs for an inline 1x1 PNG. The helper still runs and
    // still embeds an image, but nothing leaves the machine.
    if let Value::Object(map) = &mut data {
        for (key, value) in map.iter_mut() {
            if key.contains("logo") || key.contains("image") {
                *value = Value::String(PIXEL_DATA_URL.to_string());
            }
        }
    }

    let out = DocxTemplateEngine::new()
        .render(template, data)
        .await
        .unwrap_or_else(|e| panic!("{name} should render: {e}"));

    visible_text(&document_xml(&out))
}

#[tokio::test]
async fn invoice_sample_renders_its_payload() {
    let text = render_sample("invoice", "Invoice.docx").await;

    for expected in [
        "Acme Solutions LLC",
        "Jordan Martinez",
        "Pinnacle Retail Group",
        "INV-2025-0042",
        "PO-88123",
    ] {
        assert!(
            text.contains(expected),
            "invoice output should contain {expected:?}"
        );
    }

    // No unrendered placeholders should survive.
    assert!(
        !text.contains("{{"),
        "invoice output still has an unrendered expression"
    );
}

#[tokio::test]
async fn invoice_sample_expands_its_line_item_loop() {
    let text = render_sample("invoice", "Invoice.docx").await;
    let payload: Value =
        serde_json::from_slice(&std::fs::read("docs/public/samples/invoice/payload.json").unwrap())
            .unwrap();

    let items = payload["data"]["line_items"]
        .as_array()
        .expect("sample should have line_items");
    assert!(items.len() > 1, "loop test needs more than one row");

    // Every row's description must appear — this is the {{#each}} row-cloning
    // path, the most intricate part of the engine.
    for item in items {
        let desc = item["description"].as_str().unwrap();
        assert!(
            text.contains(desc),
            "line item {desc:?} missing from rendered table"
        );
    }
}

#[tokio::test]
async fn job_offer_letter_sample_renders() {
    let text = render_sample("job-offer-letter", "JobOfferLetter.docx").await;
    assert!(!text.trim().is_empty(), "output should not be blank");
    assert!(
        !text.contains("{{"),
        "job offer output still has an unrendered expression"
    );
}

#[tokio::test]
async fn sales_quote_sample_renders() {
    let text = render_sample("sales-quote", "SalesQuote.docx").await;
    assert!(!text.trim().is_empty(), "output should not be blank");
    assert!(
        !text.contains("{{"),
        "sales quote output still has an unrendered expression"
    );
}
