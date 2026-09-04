use rendrr::models::render::{OutputFormat, Render, RenderStatus};
use rendrr::models::template::{has_docx_extension, template_storage_path, Template};

// --- Render::new ---

#[test]
fn render_new_generates_uuid_v7() {
    let template_id = uuid::Uuid::now_v7();
    let render = Render::new(template_id, OutputFormat::Docx);

    // UUID v7 has version nibble = 7
    assert_eq!(render.render_id.get_version_num(), 7);
}

#[test]
fn render_new_sets_template_id() {
    let template_id = uuid::Uuid::now_v7();
    let render = Render::new(template_id, OutputFormat::Docx);
    assert_eq!(render.template_id, template_id);
}

#[test]
fn render_new_storage_path_docx() {
    let template_id = uuid::Uuid::now_v7();
    let render = Render::new(template_id, OutputFormat::Docx);
    assert_eq!(
        render.storage_path,
        format!("renders/{}.docx", render.render_id)
    );
}

#[test]
fn render_new_storage_path_pdf() {
    let template_id = uuid::Uuid::now_v7();
    let render = Render::new(template_id, OutputFormat::Pdf);
    assert_eq!(
        render.storage_path,
        format!("renders/{}.pdf", render.render_id)
    );
}

#[test]
fn render_new_initial_status_is_pending() {
    let render = Render::new(uuid::Uuid::now_v7(), OutputFormat::Docx);
    assert!(matches!(render.render_status, RenderStatus::Pending));
}

#[test]
fn render_new_initial_file_size_is_zero() {
    let render = Render::new(uuid::Uuid::now_v7(), OutputFormat::Docx);
    assert_eq!(render.file_size_bytes, 0);
}

#[test]
fn render_new_no_error_message() {
    let render = Render::new(uuid::Uuid::now_v7(), OutputFormat::Docx);
    assert!(render.error_message.is_none());
}

#[test]
fn render_new_created_at_is_recent() {
    let before = chrono::Utc::now();
    let render = Render::new(uuid::Uuid::now_v7(), OutputFormat::Docx);
    let after = chrono::Utc::now();
    assert!(render.created_at >= before);
    assert!(render.created_at <= after);
}

#[test]
fn render_new_unique_ids() {
    let tid = uuid::Uuid::now_v7();
    let r1 = Render::new(tid, OutputFormat::Docx);
    let r2 = Render::new(tid, OutputFormat::Docx);
    assert_ne!(r1.render_id, r2.render_id);
}

// --- Template::new ---

#[test]
fn template_new_generates_uuid_v7() {
    let template = Template::new(None, "test.docx".to_string(), 1024);
    assert_eq!(template.template_id.get_version_num(), 7);
}

#[test]
fn template_new_sets_filename() {
    let template = Template::new(None, "report.docx".to_string(), 2048);
    assert_eq!(template.original_filename, "report.docx");
}

#[test]
fn template_new_sets_file_size() {
    let template = Template::new(None, "test.docx".to_string(), 5000);
    assert_eq!(template.file_size_bytes, 5000);
}

#[test]
fn template_new_sets_name() {
    let template = Template::new(
        Some("Invoice Template".to_string()),
        "invoice.docx".to_string(),
        1024,
    );
    assert_eq!(template.name, "Invoice Template");
}

#[test]
fn template_new_defaults_name_to_filename() {
    let template = Template::new(None, "invoice.docx".to_string(), 1024);
    assert_eq!(template.name, "invoice.docx");
}

#[test]
fn template_new_storage_path_format() {
    let template = Template::new(None, "test.docx".to_string(), 100);
    assert_eq!(
        template.storage_path,
        format!("templates/{}.docx", template.template_id)
    );
}

#[test]
fn template_new_content_type() {
    let template = Template::new(None, "test.docx".to_string(), 100);
    assert_eq!(
        template.content_type,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    );
}

#[test]
fn template_new_uploaded_at_is_recent() {
    let before = chrono::Utc::now();
    let template = Template::new(None, "test.docx".to_string(), 100);
    let after = chrono::Utc::now();
    assert!(template.uploaded_at >= before);
    assert!(template.uploaded_at <= after);
}

#[test]
fn template_storage_path_matches_template_new() {
    let template = Template::new(None, "test.docx".to_string(), 100);
    assert_eq!(
        template.storage_path,
        template_storage_path(&template.template_id)
    );
}

#[test]
fn has_docx_extension_accepts_any_case() {
    assert!(has_docx_extension("foo.docx"));
    assert!(has_docx_extension("foo.DOCX"));
    assert!(has_docx_extension("Foo.DocX"));
}

#[test]
fn has_docx_extension_rejects_other_formats() {
    assert!(!has_docx_extension("foo.pptx"));
    assert!(!has_docx_extension("foo.txt"));
    assert!(!has_docx_extension("foo"));
    assert!(!has_docx_extension("docx"));
}

// --- OutputFormat serialization ---

#[test]
fn output_format_serializes_lowercase() {
    let docx = serde_json::to_string(&OutputFormat::Docx).unwrap();
    assert_eq!(docx, "\"docx\"");

    let pdf = serde_json::to_string(&OutputFormat::Pdf).unwrap();
    assert_eq!(pdf, "\"pdf\"");
}

#[test]
fn output_format_deserializes_lowercase() {
    let docx: OutputFormat = serde_json::from_str("\"docx\"").unwrap();
    assert!(matches!(docx, OutputFormat::Docx));

    let pdf: OutputFormat = serde_json::from_str("\"pdf\"").unwrap();
    assert!(matches!(pdf, OutputFormat::Pdf));
}

#[test]
fn output_format_rejects_unknown_variant() {
    let err = serde_json::from_str::<OutputFormat>("\"pptx\"");
    assert!(err.is_err());
}

#[test]
fn output_format_extension_and_content_type() {
    assert_eq!(OutputFormat::Docx.extension(), "docx");
    assert_eq!(OutputFormat::Pdf.extension(), "pdf");
    assert_eq!(OutputFormat::Pdf.content_type(), "application/pdf");
    assert_eq!(
        OutputFormat::Docx.content_type(),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    );
}

// --- RenderStatus serialization ---

#[test]
fn render_status_serializes_lowercase() {
    let pending = serde_json::to_string(&RenderStatus::Pending).unwrap();
    assert_eq!(pending, "\"pending\"");

    let completed = serde_json::to_string(&RenderStatus::Completed).unwrap();
    assert_eq!(completed, "\"completed\"");

    let failed = serde_json::to_string(&RenderStatus::Failed).unwrap();
    assert_eq!(failed, "\"failed\"");
}
