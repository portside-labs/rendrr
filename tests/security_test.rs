//! Tests for the hardening around caller-controlled input: template archives
//! (which are ZIPs, and therefore compressible far past their upload limit)
//! and image URLs (which are fetched by the server on the caller's behalf).

use bytes::Bytes;
use rendrr::docx::DocxTemplateEngine;
use rendrr::services::url_guard;
use serde_json::json;
use std::io::{Cursor, Write};
use zip::{write::FileOptions, ZipWriter};

/// Build a DOCX whose `word/document.xml` is `body_xml`, Deflate-compressed so
/// highly repetitive content shrinks the way a real zip bomb would.
fn docx_with_document(body_xml: &[u8]) -> Bytes {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

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
    zip.write_all(body_xml).unwrap();

    Bytes::from(zip.finish().unwrap().into_inner())
}

// ---------------- Decompression bomb ----------------

#[tokio::test]
async fn oversized_archive_entry_is_rejected_rather_than_decompressed() {
    // The upload limit bounds the *compressed* template at 25MB, which says
    // nothing about expanded size. This archive is a few hundred KB on the
    // wire and over 100MB once inflated — exactly the shape that would OOM the
    // process if entries were read without a cap.
    const PAD: usize = 101 * 1024 * 1024;

    let mut body = Vec::with_capacity(PAD + 256);
    body.extend_from_slice(
        br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>"#,
    );
    body.resize(body.len() + PAD, b'A');
    body.extend_from_slice(br#"</w:t></w:r></w:p></w:body></w:document>"#);

    let docx = docx_with_document(&body);
    assert!(
        docx.len() < 25 * 1024 * 1024,
        "bomb should fit under the upload limit, got {} bytes",
        docx.len()
    );

    let err = DocxTemplateEngine::new()
        .render(docx, json!({}))
        .await
        .expect_err("an entry expanding past the cap must be rejected");

    let msg = err.to_string();
    assert!(
        msg.contains("expands beyond"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn normal_sized_archive_entry_still_renders() {
    // The cap must not be so eager that it rejects ordinary documents.
    let docx = docx_with_document(
        br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello {{name}}</w:t></w:r></w:p></w:body></w:document>"#,
    );

    let out = DocxTemplateEngine::new()
        .render(docx, json!({"name": "Acme"}))
        .await
        .unwrap();
    assert!(!out.is_empty());
}

// ---------------- SSRF guard, end to end through the render path ----------------

async fn render_with_image_url(url: &str) -> String {
    let docx = docx_with_document(
        br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{{image logo}}</w:t></w:r></w:p></w:body></w:document>"#,
    );

    DocxTemplateEngine::new()
        .render(docx, json!({ "logo": url }))
        .await
        .expect_err("fetch should have been refused")
        .to_string()
}

#[tokio::test]
async fn render_refuses_image_url_pointing_at_cloud_metadata() {
    let msg = render_with_image_url("http://169.254.169.254/latest/meta-data/iam/").await;
    assert!(msg.contains("non-public"), "unexpected error: {msg}");
}

#[tokio::test]
async fn render_refuses_image_url_pointing_at_loopback() {
    let msg = render_with_image_url("http://127.0.0.1:9/logo.png").await;
    assert!(msg.contains("non-public"), "unexpected error: {msg}");
}

#[tokio::test]
async fn render_refuses_image_url_pointing_at_private_range() {
    let msg = render_with_image_url("http://10.0.0.5/logo.png").await;
    assert!(msg.contains("non-public"), "unexpected error: {msg}");
}

#[tokio::test]
async fn render_refuses_non_http_image_scheme() {
    let msg = render_with_image_url("file:///etc/passwd").await;
    assert!(msg.contains("scheme"), "unexpected error: {msg}");
}

#[tokio::test]
async fn render_refuses_ipv4_mapped_ipv6_bypass() {
    // `::ffff:127.0.0.1` is the same host as `127.0.0.1` written so a naive
    // string or v4-only check would miss it.
    let msg = render_with_image_url("http://[::ffff:127.0.0.1]/logo.png").await;
    assert!(msg.contains("non-public"), "unexpected error: {msg}");
}

// ---------------- Guard invariants ----------------

#[test]
fn guard_defaults_to_blocking_private_networks() {
    // The opt-out has to be opt-*out*: an operator who never sets the variable
    // must get the protective behaviour.
    if std::env::var("IMAGE_FETCH_ALLOW_PRIVATE_NETWORKS").is_err() {
        assert!(!url_guard::allow_private_networks());
    }
}

#[test]
fn guard_blocks_every_documented_range() {
    for addr in [
        "127.0.0.1",
        "10.1.2.3",
        "172.20.0.1",
        "192.168.0.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "224.0.0.1",
        "255.255.255.255",
    ] {
        let ip = addr.parse().unwrap();
        assert!(url_guard::is_blocked_ip(&ip), "{addr} should be blocked");
    }
}
