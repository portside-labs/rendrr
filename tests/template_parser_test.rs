use bytes::Bytes;
use rendrr::services::TemplateParser;
use std::io::{Cursor, Write};
use zip::{write::FileOptions, ZipWriter};

/// Helper: build a minimal valid DOCX
fn make_valid_docx() -> Bytes {
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="xml" ContentType="application/xml"/>
</Types>"#,
    )
    .unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(b"<w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:body>")
        .unwrap();

    let cursor = zip.finish().unwrap();
    Bytes::from(cursor.into_inner())
}

/// Helper: build a valid DOCX with template syntax
fn make_docx_with_content(content: &str) -> Bytes {
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="xml" ContentType="application/xml"/>
</Types>"#,
    )
    .unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(content.as_bytes()).unwrap();

    let cursor = zip.finish().unwrap();
    Bytes::from(cursor.into_inner())
}

// --- Extension validation ---

#[test]
fn validate_accepts_docx_extension() {
    let data = make_valid_docx();
    TemplateParser::validate("report.docx", &data).unwrap();
}

#[test]
fn validate_accepts_uppercase_docx_extension() {
    let data = make_valid_docx();
    TemplateParser::validate("REPORT.DOCX", &data).unwrap();
}

#[test]
fn validate_rejects_non_docx_extension() {
    let data = make_valid_docx();
    let result = TemplateParser::validate("report.pdf", &data);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("docx"), "unexpected message: {msg}");
}

#[test]
fn validate_rejects_pptx_extension() {
    // PPTX support is not part of this release — a .pptx upload must be
    // rejected on extension alone, before any structural inspection.
    let data = make_valid_docx();
    let result = TemplateParser::validate("deck.pptx", &data);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("docx"), "unexpected message: {msg}");
}

#[test]
fn validate_rejects_no_extension() {
    let data = make_valid_docx();
    let result = TemplateParser::validate("report", &data);
    assert!(result.is_err());
}

#[test]
fn validate_rejects_doc_extension() {
    let data = make_valid_docx();
    let result = TemplateParser::validate("report.doc", &data);
    assert!(result.is_err());
}

// --- Size validation ---

#[test]
fn validate_rejects_oversized_file() {
    // Create data larger than 25MB (max plan limit)
    let large_data = Bytes::from(vec![0u8; 26 * 1024 * 1024]);
    let result = TemplateParser::validate("big.docx", &large_data);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("too large") || err_msg.contains("File too large"));
}

// --- DOCX structure validation ---

#[test]
fn validate_rejects_non_zip_data() {
    let data = Bytes::from("this is not a zip file");
    let result = TemplateParser::validate("test.docx", &data);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("ZIP") || err_msg.contains("DOCX"));
}

#[test]
fn validate_rejects_zip_without_document_xml() {
    // Valid ZIP but missing word/document.xml
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(b"<Types/>").unwrap();

    // No word/document.xml!
    let cursor = zip.finish().unwrap();
    let data = Bytes::from(cursor.into_inner());

    let result = TemplateParser::validate("test.docx", &data);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("document.xml"));
}

#[test]
fn validate_rejects_zip_without_content_types() {
    // Valid ZIP but missing [Content_Types].xml
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(b"<doc/>").unwrap();

    // No [Content_Types].xml!
    let cursor = zip.finish().unwrap();
    let data = Bytes::from(cursor.into_inner());

    let result = TemplateParser::validate("test.docx", &data);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Content_Types"));
}

// --- Template syntax validation via parser ---

#[test]
fn validate_rejects_unclosed_each_block() {
    let data = make_docx_with_content(
        "<w:body><w:p><w:r><w:t>{{#each items}}no closing</w:t></w:r></w:p></w:body>",
    );
    let result = TemplateParser::validate("test.docx", &data);
    assert!(result.is_err());
}

#[test]
fn validate_accepts_valid_template_with_expressions() {
    let data = make_docx_with_content(
        "<w:body><w:p><w:r><w:t>{{name}} {{#if show}}yes{{/if}} {{#each items}}{{this}}{{/each}}</w:t></w:r></w:p></w:body>",
    );
    let result = TemplateParser::validate("test.docx", &data);
    assert!(result.is_ok());
}
