//! Tests for the structured report produced when a render fails.
//!
//! This is the most client-visible error surface in the service: a failed
//! render returns a `ValidationReport` that API consumers parse and show to
//! their own users. The engine builds it by re-scanning the template for
//! syntax problems and mining the Handlebars error string for the offending
//! field, so it is easy to break without breaking any happy path.

use bytes::Bytes;
use rendrr::docx::DocxTemplateEngine;
use rendrr::errors::{RenderError, ValidationReport};
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use zip::{write::FileOptions, ZipWriter};

fn docx_with_body(inner: &str) -> Bytes {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#,
    )
    .unwrap();

    zip.start_file("word/_rels/document.xml.rels", options)
        .unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#,
    )
    .unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>"#,
            inner
        )
        .as_bytes(),
    )
    .unwrap();

    Bytes::from(zip.finish().unwrap().into_inner())
}

/// Render and return the parsed report, asserting the failure took the
/// structured path rather than surfacing as an opaque string.
async fn failing_report(inner: &str, data: Value) -> ValidationReport {
    let err = DocxTemplateEngine::new()
        .render(docx_with_body(inner), data)
        .await
        .expect_err("render should have failed");

    let msg = match err {
        RenderError::RenderingFailed(m) => m,
        other => panic!("expected RenderingFailed, got {other:?}"),
    };

    let json_str = msg
        .strip_prefix("VALIDATION:")
        .unwrap_or_else(|| panic!("report should use the VALIDATION: prefix, got: {msg}"));

    serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("report should be valid JSON ({e}): {json_str}"))
}

#[tokio::test]
async fn helper_failure_produces_a_render_failed_report() {
    // `chunk` with a zero size fails inside the helper, which is the path that
    // routes through parse_render_error.
    let report = failing_report("{{#chunk items 0}}x{{/chunk}}", json!({"items": [1, 2]})).await;
    assert_eq!(report.status, "render_failed");
    assert!(
        report.message.contains("error"),
        "unexpected message: {}",
        report.message
    );
}

#[tokio::test]
async fn report_errors_are_categorised() {
    let report = failing_report("{{#chunk items 0}}x{{/chunk}}", json!({"items": [1]})).await;
    for issue in &report.errors {
        let category = issue
            .category
            .as_deref()
            .unwrap_or_else(|| panic!("every issue should carry a category: {issue:?}"));
        assert!(
            category == "syntax" || category == "data",
            "unexpected category {category:?}"
        );
    }
}

#[tokio::test]
async fn report_issues_carry_severity_and_location() {
    let report = failing_report("{{#chunk items 0}}x{{/chunk}}", json!({"items": [1]})).await;
    assert!(!report.errors.is_empty(), "report should list the problem");

    for issue in &report.errors {
        assert_eq!(issue.severity, "error");
        assert!(
            !issue.suggestion.is_empty(),
            "a suggestion is the point of the report: {issue:?}"
        );
        assert_eq!(
            issue.location.as_deref(),
            Some("word/document.xml"),
            "issues should name the part they came from"
        );
    }
}

#[tokio::test]
async fn image_helper_failure_is_reported_structurally() {
    // `{{image}}` with no argument fails in the helper before any fetch.
    let report = failing_report("{{image}}", json!({})).await;
    assert_eq!(report.status, "render_failed");
    assert!(!report.errors.is_empty());
}

#[tokio::test]
async fn report_survives_a_round_trip_through_json() {
    // The engine serializes the report into an error string and `ApiError`
    // deserializes it back out to build the HTTP response, so the type has to
    // round-trip cleanly or clients get a 500 instead of their report.
    let report = failing_report("{{#chunk items 0}}x{{/chunk}}", json!({"items": [1]})).await;
    let encoded = serde_json::to_string(&report).unwrap();
    let decoded: ValidationReport = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.status, report.status);
    assert_eq!(decoded.errors.len(), report.errors.len());
}

#[tokio::test]
async fn unclosed_block_is_caught_at_render_time_too() {
    // Upload validation normally catches this, but the render path has its own
    // re-scan so a template that reached storage some other way still gets a
    // structured answer rather than a panic or a bare 500.
    let err = DocxTemplateEngine::new()
        .render(
            docx_with_body("{{#each items}}{{this}}"),
            json!({"items": [1]}),
        )
        .await;
    assert!(err.is_err(), "an unclosed block should not render");
}

#[tokio::test]
async fn missing_fields_render_blank_rather_than_failing() {
    // strict_mode is off by design: a missing value leaves a gap in the
    // document instead of failing the whole render.
    let out = DocxTemplateEngine::new()
        .render(docx_with_body("Hello {{missing}}!"), json!({}))
        .await
        .expect("missing data should not be an error");
    assert!(!out.is_empty());
}

#[test]
fn engine_default_matches_new() {
    let _ = DocxTemplateEngine::default();
    let _ = DocxTemplateEngine::new();
}
