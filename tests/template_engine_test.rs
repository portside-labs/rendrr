use bytes::Bytes;
use rendrr::docx::DocxTemplateEngine as TemplateEngine;
use std::io::{Cursor, Read, Write};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

/// Helper: build a minimal valid DOCX (ZIP) with the given document.xml content
fn make_docx(document_xml: &str) -> Bytes {
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    // [Content_Types].xml (required)
    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    )
    .unwrap();

    // _rels/.rels (required by some ZIP readers)
    zip.start_file("_rels/.rels", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    )
    .unwrap();

    // word/_rels/document.xml.rels
    zip.start_file("word/_rels/document.xml.rels", options)
        .unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#,
    )
    .unwrap();

    // word/document.xml
    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();

    let cursor = zip.finish().unwrap();
    Bytes::from(cursor.into_inner())
}

/// Helper: build a DOCX with document.xml + a header file
fn make_docx_with_header(document_xml: &str, header_xml: &str) -> Bytes {
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    )
    .unwrap();

    zip.start_file("_rels/.rels", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    )
    .unwrap();

    zip.start_file("word/_rels/document.xml.rels", options)
        .unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#,
    )
    .unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();

    zip.start_file("word/header1.xml", options).unwrap();
    zip.write_all(header_xml.as_bytes()).unwrap();

    let cursor = zip.finish().unwrap();
    Bytes::from(cursor.into_inner())
}

/// Helper: extract document.xml from a rendered DOCX
fn extract_document_xml(docx_bytes: &Bytes) -> String {
    let cursor = Cursor::new(docx_bytes.to_vec());
    let mut archive = ZipArchive::new(cursor).unwrap();
    let mut file = archive.by_name("word/document.xml").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    content
}

/// Helper: extract a named file from a rendered DOCX
fn extract_file(docx_bytes: &Bytes, name: &str) -> String {
    let cursor = Cursor::new(docx_bytes.to_vec());
    let mut archive = ZipArchive::new(cursor).unwrap();
    let mut file = archive.by_name(name).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    content
}

// ==========================================================
// Escaping
// ==========================================================

#[tokio::test]
async fn render_escaped_braces_output_literal() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        r"<w:body><w:p><w:r><w:t>Hello \{{name}} and {{actual}}</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({"actual": "World"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    // \{{name}} should output literal {{name}}, not substitute
    assert!(
        xml.contains("{{name}}"),
        "Escaped braces not preserved: {}",
        xml
    );
    assert!(
        xml.contains("World"),
        "Regular variable not rendered: {}",
        xml
    );
}

// ==========================================================
// Block tag paragraph stripping
// ==========================================================

#[tokio::test]
async fn render_if_on_own_line_no_blank_paragraph() {
    let engine = TemplateEngine::new();
    // {{#if}} and {{/if}} are each in their own paragraph — should not leave empty paragraphs
    let docx = make_docx(
        "<w:body>\
            <w:p><w:r><w:t>{{#if show}}</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Visible content</w:t></w:r></w:p>\
            <w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p>\
         </w:body>",
    );
    let data = serde_json::json!({"show": true});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Visible content"));
    // The output should NOT have empty paragraphs where {{#if}} and {{/if}} were
    // Count <w:p> elements — should be exactly 1 (the content paragraph)
    let para_count = xml.matches("<w:p>").count() + xml.matches("<w:p ").count();
    assert_eq!(
        para_count, 1,
        "Expected 1 paragraph but found {} — block tags left empty paragraphs. XML: {}",
        para_count, xml
    );
}

#[tokio::test]
async fn render_if_false_on_own_line_no_blank_paragraph() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body>\
            <w:p><w:r><w:t>Before</w:t></w:r></w:p>\
            <w:p><w:r><w:t>{{#if hidden}}</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Secret content</w:t></w:r></w:p>\
            <w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p>\
            <w:p><w:r><w:t>After</w:t></w:r></w:p>\
         </w:body>",
    );
    let data = serde_json::json!({"hidden": false});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Before"));
    assert!(xml.contains("After"));
    assert!(!xml.contains("Secret content"));
    // Should have exactly 2 paragraphs: Before and After
    let para_count = xml.matches("<w:p>").count() + xml.matches("<w:p ").count();
    assert_eq!(
        para_count, 2,
        "Expected 2 paragraphs but found {} — block tags left empty paragraphs. XML: {}",
        para_count, xml
    );
}

#[tokio::test]
async fn render_inline_if_preserves_paragraph() {
    let engine = TemplateEngine::new();
    // {{#if}} used inline with other text — paragraph should NOT be stripped
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>Hello {{#if show}}World{{/if}}</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({"show": true});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Hello World"));
}

#[tokio::test]
async fn render_if_on_own_line_preserves_preceding_empty_paragraph() {
    let engine = TemplateEngine::new();
    // Empty paragraph (spacer) followed by {{#if}} on its own paragraph — spacer must be preserved
    let docx = make_docx(
        "<w:body>\
            <w:p><w:r><w:t>Content above</w:t></w:r></w:p>\
            <w:p></w:p>\
            <w:p><w:r><w:t>{{#if notes}}</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Notes: {{notes}}</w:t></w:r></w:p>\
            <w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p>\
         </w:body>",
    );
    let data = serde_json::json!({"notes": "Hello"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Content above"));
    assert!(xml.contains("Notes: Hello"));
    // Should have 3 paragraphs: content, empty spacer, notes
    let para_count = xml.matches("<w:p>").count() + xml.matches("<w:p ").count();
    assert_eq!(
        para_count, 3,
        "Expected 3 paragraphs (content + spacer + notes) but found {}. XML: {}",
        para_count, xml
    );
}

#[tokio::test]
async fn render_if_preserves_self_closing_empty_paragraph_spacer() {
    let engine = TemplateEngine::new();
    // Real-world scenario: Word uses self-closing <w:p .../> for empty paragraphs
    let docx = make_docx(
        r#"<w:body><w:p><w:r><w:t>Content above</w:t></w:r></w:p><w:p w:rsidR="00A33BA9" w:rsidRDefault="00A33BA9"/><w:p w:rsidR="00A7009C"><w:r><w:t>{{#if notes}}</w:t></w:r></w:p><w:p><w:r><w:t>Notes: {{notes}}</w:t></w:r></w:p><w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p></w:body>"#,
    );
    let data = serde_json::json!({"notes": "Hello"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Content above"));
    assert!(xml.contains("Notes: Hello"));
    // The self-closing empty paragraph must still be present as a spacer
    assert!(
        xml.contains(r#"<w:p w:rsidR="00A33BA9" w:rsidRDefault="00A33BA9"/>"#),
        "Self-closing empty paragraph spacer was eaten. XML: {}",
        xml
    );
}

// ==========================================================
// Conditional table rows
// ==========================================================

#[tokio::test]
async fn render_if_controls_table_rows() {
    let engine = TemplateEngine::new();
    // {{#if}} and {{/if}} on their own table rows should control the rows between them
    let docx = make_docx(
        "<w:body><w:tbl>\
            <w:tr><w:tc><w:p><w:r><w:t>Always visible</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>{{#if show_bonus}}</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>Bonus: {{bonus}}</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p></w:tc></w:tr>\
        </w:tbl></w:body>",
    );
    let data = serde_json::json!({"show_bonus": true, "bonus": "$5,000"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Always visible"));
    assert!(xml.contains("Bonus: $5,000"));
}

#[tokio::test]
async fn render_if_false_hides_table_rows() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:tbl>\
            <w:tr><w:tc><w:p><w:r><w:t>Always visible</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>{{#if show_bonus}}</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>Bonus: {{bonus}}</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p></w:tc></w:tr>\
        </w:tbl></w:body>",
    );
    let data = serde_json::json!({"show_bonus": false, "bonus": "$5,000"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Always visible"));
    assert!(!xml.contains("Bonus"));
}

#[tokio::test]
async fn render_if_else_controls_table_rows() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:tbl>\
            <w:tr><w:tc><w:p><w:r><w:t>{{#if is_vip}}</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>VIP Row</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>{{else}}</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>Standard Row</w:t></w:r></w:p></w:tc></w:tr>\
            <w:tr><w:tc><w:p><w:r><w:t>{{/if}}</w:t></w:r></w:p></w:tc></w:tr>\
        </w:tbl></w:body>",
    );

    // Test with is_vip = true
    let data = serde_json::json!({"is_vip": true});
    let result = engine.render(docx.clone(), data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("VIP Row"));
    assert!(!xml.contains("Standard Row"));

    // Test with is_vip = false
    let data = serde_json::json!({"is_vip": false});
    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(!xml.contains("VIP Row"));
    assert!(xml.contains("Standard Row"));
}

// ==========================================================
// Basic rendering
// ==========================================================

#[tokio::test]
async fn render_simple_variable() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>Hello {{name}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"name": "World"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Hello World"));
    assert!(!xml.contains("{{name}}"));
}

#[tokio::test]
async fn render_multiple_variables() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{first}} {{last}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"first": "John", "last": "Doe"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("John Doe"));
}

#[tokio::test]
async fn render_missing_variable_renders_empty() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>Hello {{name}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({});

    // strict_mode is false, so missing vars render as empty string
    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Hello "));
    assert!(!xml.contains("{{name}}"));
}

#[tokio::test]
async fn render_nested_object_path() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{user.address.city}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"user": {"address": {"city": "Portland"}}});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Portland"));
}

// ==========================================================
// Each loops
// ==========================================================

#[tokio::test]
async fn render_each_loop() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#each items}}{{this}} {{/each}}</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({"items": ["a", "b", "c"]});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("a b c"));
}

#[tokio::test]
async fn render_each_with_object_properties() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#each people}}{{name}},{{/each}}</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({"people": [{"name": "Alice"}, {"name": "Bob"}]});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Alice,Bob,"));
}

// ==========================================================
// Conditionals
// ==========================================================

#[tokio::test]
async fn render_if_true() {
    let engine = TemplateEngine::new();
    let docx =
        make_docx("<w:body><w:p><w:r><w:t>{{#if show}}visible{{/if}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"show": true});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("visible"));
}

#[tokio::test]
async fn render_if_false() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>before{{#if show}}hidden{{/if}}after</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({"show": false});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(!xml.contains("hidden"));
    assert!(xml.contains("beforeafter"));
}

// ==========================================================
// XML normalization (merging split runs)
// ==========================================================

#[tokio::test]
async fn render_split_variable_across_runs() {
    let engine = TemplateEngine::new();
    // Word often splits {{name}} into separate <w:t> elements: {{, name, }}
    let docx = make_docx(
        r#"<w:body><w:p><w:r><w:t>{{</w:t></w:r><w:r><w:t>name</w:t></w:r><w:r><w:t>}}</w:t></w:r></w:p></w:body>"#,
    );
    let data = serde_json::json!({"name": "Merged"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Merged"));
}

#[tokio::test]
async fn render_split_variable_within_same_run() {
    let engine = TemplateEngine::new();
    // Split <w:t> elements within the same <w:r>
    let docx = make_docx(
        r#"<w:body><w:p><w:r><w:t>{{na</w:t><w:t xml:space="preserve">me}}</w:t></w:r></w:p></w:body>"#,
    );
    let data = serde_json::json!({"name": "Fixed"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Fixed"));
}

#[tokio::test]
async fn render_removes_proof_error_markers() {
    let engine = TemplateEngine::new();
    // Word inserts proofErr elements that can split expressions
    let docx = make_docx(
        r#"<w:body><w:p><w:r><w:t>{{na</w:t></w:r><w:proofErr w:type="spellStart"/><w:r><w:t>me}}</w:t></w:r></w:p></w:body>"#,
    );
    let data = serde_json::json!({"name": "Cleaned"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Cleaned"));
}

// ==========================================================
// Headers and footers
// ==========================================================

#[tokio::test]
async fn render_header_with_variables() {
    let engine = TemplateEngine::new();
    let docx = make_docx_with_header(
        "<w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p></w:body>",
        "<w:hdr><w:p><w:r><w:t>Header: {{title}}</w:t></w:r></w:p></w:hdr>",
    );
    let data = serde_json::json!({"title": "Report"});

    let result = engine.render(docx, data).await.unwrap();
    let header = extract_file(&result, "word/header1.xml");
    assert!(header.contains("Header: Report"));
}

// ==========================================================
// Data validation
// ==========================================================

#[tokio::test]
async fn render_rejects_deeply_nested_data() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{x}}</w:t></w:r></w:p></w:body>");

    // Build data nested 150 levels deep (limit is 100)
    let mut data = serde_json::json!("leaf");
    for _ in 0..150 {
        data = serde_json::json!({"nested": data});
    }

    let result = engine.render(docx, data).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("depth"));
}

#[tokio::test]
async fn render_rejects_oversized_array() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{x}}</w:t></w:r></w:p></w:body>");

    // Array with 10001 elements (limit is 10000)
    let large_array: Vec<i32> = (0..10001).collect();
    let data = serde_json::json!({"items": large_array});

    let result = engine.render(docx, data).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Array size"));
}

#[tokio::test]
async fn render_accepts_valid_depth_data() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{a.b.c}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"a": {"b": {"c": "deep"}}});

    let result = engine.render(docx, data).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn render_accepts_array_at_limit() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>ok</w:t></w:r></w:p></w:body>");

    let array: Vec<i32> = (0..10000).collect();
    let data = serde_json::json!({"items": array});

    let result = engine.render(docx, data).await;
    assert!(result.is_ok());
}

// ==========================================================
// Template syntax validation
// ==========================================================

#[test]
fn validate_syntax_valid_template() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>Hello {{name}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn validate_syntax_each_block() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#each items}}{{this}}{{/each}}</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn validate_syntax_if_block() {
    let engine = TemplateEngine::new();
    let docx =
        make_docx("<w:body><w:p><w:r><w:t>{{#if show}}visible{{/if}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn validate_syntax_unclosed_each() {
    let engine = TemplateEngine::new();
    let docx =
        make_docx("<w:body><w:p><w:r><w:t>{{#each items}}content</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(!report.errors.is_empty());
    assert!(report
        .errors
        .iter()
        .any(|e| e.error_type == "UnclosedBlock"));
}

#[test]
fn validate_syntax_unclosed_if() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{#if x}}content</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
}

#[test]
fn validate_syntax_unmatched_closing_each() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>content{{/each}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(report
        .errors
        .iter()
        .any(|e| e.error_type == "UnmatchedClosingTag"));
}

#[test]
fn validate_syntax_mismatched_blocks() {
    let engine = TemplateEngine::new();
    // Opening with #each but closing with /if
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#each items}}content{{/if}}</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(report
        .errors
        .iter()
        .any(|e| e.error_type == "MismatchedClosingTag"));
}

#[test]
fn validate_syntax_chunk_without_size() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#chunk items}}content{{/chunk}}</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(report
        .errors
        .iter()
        .any(|e| e.error_type == "MissingParameter"));
}

#[test]
fn validate_syntax_chunk_with_size_is_valid() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#chunk items 3}}{{this}}{{/chunk}}</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn validate_syntax_invalid_zip() {
    let engine = TemplateEngine::new();
    let bad_data = Bytes::from("not a zip file");
    let result = engine.validate_template_syntax(&bad_data);
    assert!(result.is_err());
}

#[test]
fn validate_syntax_no_expressions_is_valid() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>Just plain text</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn render_split_variable_across_runs_with_formatting() {
    // Simulates Word splitting {{name}} across runs with <w:rPr> content
    // (not just self-closing <w:rPr/>), as happens with different rsidR values
    let engine = TemplateEngine::new();
    let docx = make_docx(
        r#"<w:body><w:p><w:r><w:rPr><w:sz w:val="18"/></w:rPr><w:t>{{</w:t></w:r><w:r w:rsidR="001"><w:rPr><w:sz w:val="18"/></w:rPr><w:t>name</w:t></w:r><w:r w:rsidR="002"><w:rPr><w:sz w:val="18"/></w:rPr><w:t>}}</w:t></w:r></w:p></w:body>"#,
    );
    let data = serde_json::json!({"name": "Alice"});
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(engine.render(docx, data));
    assert!(result.is_ok(), "Render failed: {:?}", result.err());
}

#[test]
fn validate_syntax_image_non_numeric_width_rejected() {
    let engine = TemplateEngine::new();
    let docx =
        make_docx("<w:body><w:p><w:r><w:t>{{image logo width=abc}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.error_type == "InvalidParameter"),
        "Expected InvalidParameter error, got: {:?}",
        report.errors
    );
}

#[test]
fn validate_syntax_image_numeric_width_accepted() {
    let engine = TemplateEngine::new();
    let docx =
        make_docx("<w:body><w:p><w:r><w:t>{{image logo width=160}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn validate_syntax_image_no_width_accepted() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{image logo}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_ok());
}

#[test]
fn validate_syntax_literal_braces_no_false_positive() {
    let engine = TemplateEngine::new();
    // Literal text with braces like "Tax ({{rate}})" should not trigger MismatchedBraces
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>Tax ({{tax_rate}}) and {{name}}</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(
        result.is_ok(),
        "Literal braces caused false positive: {:?}",
        result
    );
}

#[test]
fn validate_syntax_mismatched_braces_detected() {
    let engine = TemplateEngine::new();
    // {{bank_name} is missing one closing brace
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>Hello {{name}} your bank is {{bank_name}</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.error_type == "MismatchedBraces"),
        "Expected MismatchedBraces error, got: {:?}",
        report.errors
    );
}

#[test]
fn validate_syntax_multiple_errors_detected() {
    let engine = TemplateEngine::new();
    // Two errors: unclosed {{#each}} (missing /each) AND unclosed {{#if}} (missing /if)
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#each items}}{{name}}{{each}}{{#if active}}hello</w:t></w:r></w:p></w:body>",
    );
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    let unclosed_count = report
        .errors
        .iter()
        .filter(|e| e.error_type == "UnclosedBlock")
        .count();
    assert!(
        unclosed_count >= 2,
        "Expected at least 2 UnclosedBlock errors (each + if), got {}: {:?}",
        unclosed_count,
        report.errors
    );
}

#[test]
fn validate_syntax_unclosed_if_detected() {
    let engine = TemplateEngine::new();
    let docx =
        make_docx("<w:body><w:p><w:r><w:t>{{#if active}}hello{{name}}</w:t></w:r></w:p></w:body>");
    let result = engine.validate_template_syntax(&docx);
    assert!(result.is_err());
    let report = result.unwrap_err();
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.error_type == "UnclosedBlock"),
        "Expected UnclosedBlock error for unclosed #if, got: {:?}",
        report.errors
    );
}

// ==========================================================
// Chunk helper
// ==========================================================

#[tokio::test]
async fn render_chunk_helper() {
    let engine = TemplateEngine::new();
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#chunk items 2}}[{{#each this}}{{this}}{{/each}}]{{/chunk}}</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({"items": [1, 2, 3, 4, 5]});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    // Chunks of 2: [1,2], [3,4], [5]
    assert!(xml.contains("[12]"));
    assert!(xml.contains("[34]"));
    assert!(xml.contains("[5]"));
}

#[tokio::test]
async fn render_chunk_nested_in_each_preserves_parent_scope() {
    let engine = TemplateEngine::new();
    // Simulates the inspection report pattern: {{#each sections}} > {{#each items}} > {{#chunk images 2}}
    let docx = make_docx(
        "<w:body><w:p><w:r><w:t>{{#each sections}}[{{name}}:{{#each items}}{{#chunk images 2}}({{#each this}}{{this}}{{/each}}){{/chunk}}{{/each}}]{{/each}}</w:t></w:r></w:p></w:body>",
    );
    let data = serde_json::json!({
        "sections": [
            {
                "name": "A",
                "items": [
                    {"images": ["img1", "img2", "img3"]}
                ]
            },
            {
                "name": "B",
                "items": [
                    {"images": ["img4"]}
                ]
            }
        ]
    });

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    // Section A: chunk of 2 then chunk of 1
    assert!(xml.contains("[A:(img1img2)(img3)]"), "Got: {}", xml);
    // Section B: single chunk
    assert!(xml.contains("[B:(img4)]"), "Got: {}", xml);
}

// ==========================================================
// Output is valid ZIP
// ==========================================================

#[tokio::test]
async fn render_output_is_valid_zip() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{x}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"x": "test"});

    let result = engine.render(docx, data).await.unwrap();
    let cursor = Cursor::new(result.to_vec());
    let archive = ZipArchive::new(cursor);
    assert!(archive.is_ok());

    let mut archive = archive.unwrap();
    // Must contain document.xml
    assert!(archive.by_name("word/document.xml").is_ok());
    // Must contain [Content_Types].xml
    assert!(archive.by_name("[Content_Types].xml").is_ok());
}

#[tokio::test]
async fn render_preserves_non_template_files() {
    let engine = TemplateEngine::new();

    // Build a DOCX with an extra file
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#).unwrap();

    zip.start_file("word/_rels/document.xml.rels", options)
        .unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#).unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(b"<w:body><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:body>")
        .unwrap();

    zip.start_file("word/styles.xml", options).unwrap();
    zip.write_all(b"<w:styles>keep me</w:styles>").unwrap();

    let cursor = zip.finish().unwrap();
    let docx = Bytes::from(cursor.into_inner());

    let result = engine.render(docx, serde_json::json!({})).await.unwrap();
    let styles = extract_file(&result, "word/styles.xml");
    assert!(styles.contains("keep me"));
}

// ==========================================================
// Special characters
// ==========================================================

#[tokio::test]
async fn render_html_entities_in_data() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{content}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"content": "A & B < C > D"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    // Handlebars HTML-escapes by default
    assert!(xml.contains("&amp;") || xml.contains("A & B"));
}

#[tokio::test]
async fn render_unicode_in_data() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>{{text}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"text": "Caf\u{00e9} \u{1f600}"});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Caf\u{00e9}"));
}

// ==========================================================
// Table loop normalization
// ==========================================================

#[tokio::test]
async fn render_table_with_each_in_rows() {
    let engine = TemplateEngine::new();
    // Table where {{#each}} and {{/each}} are in their own rows
    let docx = make_docx(
        r#"<w:body><w:tbl>
<w:tr><w:tc><w:p><w:r><w:t>{{#each rows}}</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>{{name}}</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>{{/each}}</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl></w:body>"#,
    );
    let data = serde_json::json!({"rows": [{"name": "Alice"}, {"name": "Bob"}]});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Alice"));
    assert!(xml.contains("Bob"));
}

// ==========================================================
// Ensure image content types
// ==========================================================

#[tokio::test]
async fn render_empty_data_no_crash() {
    let engine = TemplateEngine::new();
    let docx = make_docx("<w:body><w:p><w:r><w:t>Static content only</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({});

    let result = engine.render(docx, data).await.unwrap();
    let xml = extract_document_xml(&result);
    assert!(xml.contains("Static content only"));
}

// ==========================================================
// Header image rendering
// ==========================================================

/// Helper: build a DOCX with a header that has its own (self-closing) rels file
fn make_docx_with_header_rels(document_xml: &str, header_xml: &str) -> Bytes {
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    )
    .unwrap();

    zip.start_file("_rels/.rels", options).unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    )
    .unwrap();

    zip.start_file("word/_rels/document.xml.rels", options)
        .unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
</Relationships>"#,
    )
    .unwrap();

    // Self-closing rels file for the header (common in real-world DOCX files)
    zip.start_file("word/_rels/header1.xml.rels", options)
        .unwrap();
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#,
    )
    .unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();

    zip.start_file("word/header1.xml", options).unwrap();
    zip.write_all(header_xml.as_bytes()).unwrap();

    let cursor = zip.finish().unwrap();
    Bytes::from(cursor.into_inner())
}

/// Minimal valid 1x1 red pixel PNG as a base64 data URI for testing
fn base64_png_data_uri() -> String {
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC".to_string()
}

#[tokio::test]
async fn render_image_in_header_creates_header_rels() {
    let engine = TemplateEngine::new();

    let docx = make_docx_with_header_rels(
        "<w:body><w:p><w:r><w:t>Body text</w:t></w:r></w:p></w:body>",
        "<w:hdr><w:p><w:r><w:t>{{image logo}}</w:t></w:r></w:p></w:hdr>",
    );
    let data = serde_json::json!({"logo": base64_png_data_uri()});

    let result = engine.render(docx, data).await.unwrap();
    let result_bytes = Bytes::from(result.to_vec());

    // header1.xml should contain drawing XML (not the placeholder)
    let header = extract_file(&result_bytes, "word/header1.xml");
    assert!(
        header.contains("<w:drawing"),
        "Header should contain drawing XML"
    );
    assert!(
        !header.contains("IMAGE_PLACEHOLDER"),
        "Placeholder should be replaced"
    );

    // header1.xml.rels should contain the image relationship
    let header_rels = extract_file(&result_bytes, "word/_rels/header1.xml.rels");
    assert!(
        header_rels.contains("relationships/image"),
        "Header rels should contain image relationship"
    );
    assert!(
        header_rels.contains("media/"),
        "Header rels should reference media file"
    );

    // document.xml.rels should NOT contain the image relationship
    let doc_rels = extract_file(&result_bytes, "word/_rels/document.xml.rels");
    assert!(
        !doc_rels.contains("relationships/image"),
        "Document rels should NOT contain header's image relationship"
    );
}

#[tokio::test]
async fn render_image_in_body_uses_document_rels() {
    let engine = TemplateEngine::new();

    let docx = make_docx("<w:body><w:p><w:r><w:t>{{image logo}}</w:t></w:r></w:p></w:body>");
    let data = serde_json::json!({"logo": base64_png_data_uri()});

    let result = engine.render(docx, data).await.unwrap();

    let cursor = Cursor::new(result.to_vec());
    let mut archive = ZipArchive::new(cursor).unwrap();

    // document.xml should contain drawing XML
    let doc_xml = extract_document_xml(&Bytes::from(result.to_vec()));
    assert!(
        doc_xml.contains("<w:drawing"),
        "Body should contain drawing XML"
    );

    // document.xml.rels should contain the image relationship
    let mut doc_rels_file = archive.by_name("word/_rels/document.xml.rels").unwrap();
    let mut doc_rels = String::new();
    doc_rels_file.read_to_string(&mut doc_rels).unwrap();
    assert!(
        doc_rels.contains("relationships/image"),
        "Document rels should contain image relationship"
    );
}
