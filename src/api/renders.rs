use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    docx::DocxTemplateEngine,
    errors::{ApiError, TemplateError, ValidationIssue, ValidationReport},
    models::{render::Render, template::template_storage_path, OutputFormat},
    oauth::{AuthClaims, Scope},
    AppState,
};

// Maximum JSON payload size: 10MB
const MAX_JSON_SIZE: usize = 10 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct RenderRequest {
    pub template_id: Option<String>,
    pub data: serde_json::Value,
    pub output_format: OutputFormat,
}

#[derive(Debug, Serialize)]
pub struct RenderResponse {
    pub render_id: String,
    pub template_id: String,
    pub output_format: String,
    pub storage_path: String,
    pub created_at: String,
    pub render_status: String,
}

impl From<Render> for RenderResponse {
    fn from(render: Render) -> Self {
        let output_format = render.output_format.extension().to_string();

        let render_status = match render.render_status {
            crate::models::render::RenderStatus::Pending => "pending".to_string(),
            crate::models::render::RenderStatus::Completed => "completed".to_string(),
            crate::models::render::RenderStatus::Failed => "failed".to_string(),
        };

        Self {
            render_id: render.render_id.to_string(),
            template_id: render.template_id.to_string(),
            output_format,
            storage_path: render.storage_path,
            created_at: render.created_at.to_rfc3339(),
            render_status,
        }
    }
}

fn not_found_report(template_id: &str) -> ValidationReport {
    ValidationReport {
        status: "validation_failed".to_string(),
        message: format!("Template not found: {}", template_id),
        errors: vec![ValidationIssue {
            error_type: "not_found".to_string(),
            expression: "template_id".to_string(),
            suggestion: "Provide a valid template_id that exists in storage".to_string(),
            severity: "error".to_string(),
            category: None,
            location: None,
        }],
    }
}

/// POST /v1/renders — Render a document from a stored template.
pub async fn render_document(
    State(state): State<AppState>,
    auth: AuthClaims,
    Json(request): Json<RenderRequest>,
) -> Result<(StatusCode, Json<RenderResponse>), ApiError> {
    auth.require_scope(Scope::RendersWrite)?;

    let template_id = request.template_id.ok_or_else(|| {
        ApiError::Template(TemplateError::ValidationFailed(ValidationReport {
            status: "validation_failed".to_string(),
            message: "Missing required field: template_id".to_string(),
            errors: vec![ValidationIssue {
                error_type: "missing_field".to_string(),
                expression: "template_id".to_string(),
                suggestion: "Provide a template_id in the request body".to_string(),
                severity: "error".to_string(),
                category: None,
                location: None,
            }],
        }))
    })?;

    let data_size = serde_json::to_string(&request.data)
        .map_err(|e| ApiError::BadRequest(format!("Invalid JSON: {}", e)))?
        .len();

    if data_size > MAX_JSON_SIZE {
        return Err(ApiError::BadRequest(format!(
            "JSON payload too large: {} bytes (max {} bytes)",
            data_size, MAX_JSON_SIZE
        )));
    }

    let template_uuid = Uuid::parse_str(&template_id).map_err(|_| {
        ApiError::Template(TemplateError::ValidationFailed(ValidationReport {
            status: "validation_failed".to_string(),
            message: "Invalid template_id format".to_string(),
            errors: vec![ValidationIssue {
                error_type: "invalid_field".to_string(),
                expression: "template_id".to_string(),
                suggestion: "template_id must be a valid UUID".to_string(),
                severity: "error".to_string(),
                category: None,
                location: None,
            }],
        }))
    })?;

    let template_data = match state
        .template_storage
        .get(&template_storage_path(&template_uuid))
        .await
    {
        Ok(data) => data,
        Err(crate::errors::StorageError::FileNotFound(_)) => {
            return Err(ApiError::Template(TemplateError::ValidationFailed(
                not_found_report(&template_id),
            )));
        }
        Err(other) => return Err(ApiError::Storage(other)),
    };

    let rendered_docx = DocxTemplateEngine::new()
        .render(template_data, request.data)
        .await
        .map_err(ApiError::Render)?;

    let rendered_data = match request.output_format {
        OutputFormat::Docx => rendered_docx,
        OutputFormat::Pdf => {
            let pdf = state.pdf_engine.as_ref().ok_or_else(|| {
                ApiError::BadRequest("PDF output is not enabled on this server.".to_string())
            })?;
            pdf.convert_docx_to_pdf(rendered_docx)
                .await
                .map_err(ApiError::Render)?
        }
    };

    let mut render = Render::new(template_uuid, request.output_format);
    render.file_size_bytes = rendered_data.len() as u64;

    state
        .render_storage
        .put(&render.storage_path, rendered_data)
        .await
        .map_err(ApiError::Storage)?;

    render.render_status = crate::models::render::RenderStatus::Completed;

    let response = RenderResponse::from(render);
    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /v1/renders/{render_id}/download — Stream a previously rendered document.
///
/// Without a metadata store, we infer the output format from object existence:
/// look for `.docx` first, then `.pdf`.
pub async fn download_render(
    State(state): State<AppState>,
    auth: AuthClaims,
    Path(render_id): Path<String>,
) -> Result<Response, ApiError> {
    auth.require_scope(Scope::RendersRead)?;

    Uuid::parse_str(&render_id)
        .map_err(|_| ApiError::BadRequest("Invalid render_id format".to_string()))?;

    let mut found = None;
    for format in [OutputFormat::Docx, OutputFormat::Pdf] {
        let path = format!("renders/{}.{}", render_id, format.extension());
        let exists =
            state.render_storage.exists(&path).await.map_err(|e| {
                ApiError::Internal(format!("Failed to check render existence: {}", e))
            })?;
        if exists {
            found = Some((path, format.content_type(), format.extension()));
            break;
        }
    }

    let (storage_path, content_type, extension) =
        found.ok_or_else(|| ApiError::BadRequest(format!("Render not found: {}", render_id)))?;

    let data = state
        .render_storage
        .get(&storage_path)
        .await
        .map_err(ApiError::Storage)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.{}\"", render_id, extension),
        )
        .body(axum::body::Body::from(data))
        .map_err(|e| ApiError::Internal(format!("Failed to build response: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::render::RenderStatus;

    #[test]
    fn render_response_serializes_pdf_output_format() {
        let mut render = Render::new(Uuid::now_v7(), OutputFormat::Pdf);
        render.render_status = RenderStatus::Completed;
        let resp = RenderResponse::from(render);
        assert_eq!(resp.output_format, "pdf");
        assert_eq!(resp.render_status, "completed");
    }

    #[test]
    fn render_response_serializes_pending_status() {
        let mut render = Render::new(Uuid::now_v7(), OutputFormat::Docx);
        render.render_status = RenderStatus::Pending;
        let resp = RenderResponse::from(render);
        assert_eq!(resp.render_status, "pending");
    }

    #[test]
    fn render_response_serializes_failed_status() {
        let mut render = Render::new(Uuid::now_v7(), OutputFormat::Docx);
        render.render_status = RenderStatus::Failed;
        let resp = RenderResponse::from(render);
        assert_eq!(resp.render_status, "failed");
    }
}
