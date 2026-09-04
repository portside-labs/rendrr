//! Integration tests for the HTTP handlers. Exercises the *real* router from
//! `rendrr::build_router` — not a copy of the route table — backed by
//! `InMemory` object stores so we don't need MinIO/S3 at test time.

use std::io::{Cursor, Write};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use bytes::Bytes;
use object_store::memory::InMemory;
use rendrr::docx::PdfEngine;
use rendrr::services::StorageClient;
use rendrr::{build_router, AppState};
use serde_json::{json, Value};
use tower::ServiceExt;
use zip::{write::FileOptions, ZipWriter};

fn minimal_docx(document_xml: &str) -> Bytes {
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
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#,
    )
    .unwrap();

    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();

    let cursor = zip.finish().unwrap();
    Bytes::from(cursor.into_inner())
}

fn build_app(pdf: bool) -> (Router, AppState) {
    let template_storage = StorageClient::from_store(Arc::new(InMemory::new()));
    let render_storage = StorageClient::from_store(Arc::new(InMemory::new()));
    let state = AppState {
        template_storage,
        render_storage,
        pdf_engine: pdf.then(PdfEngine::new),
        oauth: None,
    };
    let app = build_router(state.clone(), tower_http::cors::CorsLayer::permissive());
    (app, state)
}

const BOUNDARY: &str = "boundary123abc";

fn multipart_with_file(filename: &str, content: &[u8]) -> Body {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", BOUNDARY).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", BOUNDARY).as_bytes());
    Body::from(body)
}

fn multipart_with_file_and_name(filename: &str, content: &[u8], name: &str) -> Body {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", BOUNDARY).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}\r\n", BOUNDARY).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", BOUNDARY).as_bytes());
    Body::from(body)
}

fn empty_multipart() -> Body {
    Body::from(format!("--{}--\r\n", BOUNDARY))
}

async fn body_json(body: Body) -> Value {
    let bytes = to_bytes(body, 10 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ---------------- Upload ----------------

#[tokio::test]
async fn upload_returns_201_with_template_id() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body><w:p><w:r><w:t>{{name}}</w:t></w:r></w:p></w:body>");
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file("hello.docx", &docx))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert!(body["template_id"].is_string());
    assert_eq!(body["original_filename"], "hello.docx");
    assert_eq!(body["name"], "hello.docx");
    assert!(body["file_size_bytes"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn upload_honors_custom_name_field() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body></w:body>");
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file_and_name(
            "hello.docx",
            &docx,
            "My Cool Template",
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["name"], "My Cool Template");
}

#[tokio::test]
async fn upload_rejects_missing_file() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(empty_multipart())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_rejects_invalid_docx_extension() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file("hello.txt", b"not docx"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // The parser rejects via TemplateError, mapped to 400 by IntoResponse.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upload_rejects_garbage_docx_payload() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file("hello.docx", b"not a zip"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------- Render ----------------

async fn upload_helper(app: Router, docx: Bytes) -> String {
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file("t.docx", &docx))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    body["template_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn render_docx_succeeds() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body><w:p><w:r><w:t>Hi {{name}}</w:t></w:r></w:p></w:body>");
    let id = upload_helper(app.clone(), docx).await;

    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": id,
                "data": { "name": "World" },
                "output_format": "docx",
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["output_format"], "docx");
    assert_eq!(body["render_status"], "completed");
    assert!(body["render_id"].is_string());
}

#[tokio::test]
async fn render_missing_template_id_returns_422() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "data": {}, "output_format": "docx" }).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn render_invalid_template_id_format_returns_422() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": "not-a-uuid",
                "data": {},
                "output_format": "docx"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn render_missing_template_returns_422() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": "01234567-89ab-7def-8000-000000000000",
                "data": {},
                "output_format": "docx"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn render_pdf_without_engine_returns_400() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body>");
    let id = upload_helper(app.clone(), docx).await;

    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": id,
                "data": {},
                "output_format": "pdf"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------- Download ----------------

#[tokio::test]
async fn download_returns_rendered_docx() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body><w:p><w:r><w:t>{{name}}</w:t></w:r></w:p></w:body>");
    let id = upload_helper(app.clone(), docx).await;

    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": id,
                "data": { "name": "you" },
                "output_format": "docx"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let render_body = body_json(resp.into_body()).await;
    let render_id = render_body["render_id"].as_str().unwrap();

    let req = Request::builder()
        .uri(format!("/v1/renders/{}/download", render_id))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("wordprocessingml"));
    let cd = resp
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains(".docx"));
    let bytes = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    assert!(!bytes.is_empty());
}

#[tokio::test]
async fn download_invalid_uuid_returns_400() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/renders/not-a-uuid/download")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn download_missing_render_returns_400() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/renders/01234567-89ab-7def-8000-000000000000/download")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------- Delete ----------------

#[tokio::test]
async fn delete_existing_template_returns_204() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body></w:body>");
    let id = upload_helper(app.clone(), docx).await;

    let req = Request::builder()
        .uri(format!("/v1/templates/{}", id))
        .method("DELETE")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_missing_template_returns_404() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/templates/01234567-89ab-7def-8000-000000000000")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_invalid_uuid_returns_400() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/templates/not-a-uuid")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------- PDF path ----------------

const SAMPLE_SALES_QUOTE: &[u8] =
    include_bytes!("../docs/public/samples/sales-quote/SalesQuote.docx");

#[tokio::test]
async fn render_pdf_then_download_returns_pdf() {
    let (app, _) = build_app(true);
    // Upload a real Word fixture so dxpdf has a fully-formed document to render.
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file("SalesQuote.docx", SAMPLE_SALES_QUOTE))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    let template_id = body["template_id"].as_str().unwrap().to_string();

    // Load the matching payload (no external image URLs) and render as PDF.
    let payload_bytes =
        std::fs::read("docs/public/samples/sales-quote/payload.json").expect("payload fixture");
    let payload: Value = serde_json::from_slice(&payload_bytes).unwrap();
    let data = payload.get("data").cloned().unwrap_or(payload);
    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": template_id,
                "data": data,
                "output_format": "pdf"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["output_format"], "pdf");
    let render_id = body["render_id"].as_str().unwrap();

    let req = Request::builder()
        .uri(format!("/v1/renders/{}/download", render_id))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/pdf");
    let bytes = to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---------------- Unknown multipart field ----------------

#[tokio::test]
async fn upload_ignores_unknown_multipart_field() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body></w:body>");
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", BOUNDARY).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"some_unknown_field\"\r\n\r\n");
    body.extend_from_slice(b"ignored value\r\n");
    body.extend_from_slice(format!("--{}\r\n", BOUNDARY).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"t.docx\"\r\n\r\n",
    );
    body.extend_from_slice(&docx);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", BOUNDARY).as_bytes());

    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ---------------- JSON size limit ----------------

#[tokio::test]
async fn render_rejects_oversized_payload() {
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body></w:body>");
    let id = upload_helper(app.clone(), docx).await;

    // Build a JSON value larger than the 10 MB cap.
    let blob = "x".repeat(11 * 1024 * 1024);
    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": id,
                "data": { "x": blob },
                "output_format": "docx"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------- Non-DOCX formats are rejected ----------------

#[tokio::test]
async fn upload_pptx_returns_400() {
    // PPTX support is not in this release. The extension check must reject the
    // upload before any structural inspection, whatever the bytes contain.
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body><w:p><w:r><w:t>{{name}}</w:t></w:r></w:p></w:body>");
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", BOUNDARY),
        )
        .body(multipart_with_file("deck.pptx", &docx))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn render_with_pptx_output_format_is_rejected() {
    // `pptx` is no longer a variant of OutputFormat, so the request body fails
    // to deserialize and axum rejects it before the handler runs.
    let (app, _) = build_app(false);
    let docx = minimal_docx("<w:body><w:p><w:r><w:t>{{name}}</w:t></w:r></w:p></w:body>");
    let id = upload_helper(app.clone(), docx).await;

    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "template_id": id,
                "data": {"name": "Acme"},
                "output_format": "pptx"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------- Health ----------------

#[tokio::test]
async fn health_returns_ok_and_version() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["oauth_enabled"], false);
    assert_eq!(body["pdf_enabled"], false);
}

#[tokio::test]
async fn health_reports_pdf_engine_state() {
    let (app, _) = build_app(true);
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["pdf_enabled"], true);
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let (app, _) = build_app(false);
    let req = Request::builder()
        .uri("/v1/nope")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
