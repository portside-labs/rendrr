use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::{
    errors::ApiError,
    models::{template::template_storage_path, Template},
    oauth::{AuthClaims, Scope},
    services::TemplateParser,
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateUploadResponse {
    pub template_id: String,
    pub name: String,
    pub uploaded_at: String,
    pub original_filename: String,
    pub file_size_bytes: u64,
}

/// POST /v1/templates — Upload a new document template
pub async fn upload_template(
    State(state): State<AppState>,
    auth: AuthClaims,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TemplateUploadResponse>), ApiError> {
    auth.require_scope(Scope::TemplatesWrite)?;

    let mut file_data: Option<Bytes> = None;
    let mut filename: Option<String> = None;
    let mut template_name: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Multipart field error: {}", e);
        ApiError::BadRequest(format!("Multipart error: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                file_data = Some(field.bytes().await.map_err(|e| {
                    tracing::error!("Failed to read file bytes: {}", e);
                    ApiError::BadRequest(format!("Failed to read file: {}", e))
                })?);
            }
            "name" => {
                let value = field.text().await.map_err(|e| {
                    ApiError::BadRequest(format!("Failed to read name field: {}", e))
                })?;
                let value = value.trim().to_string();
                if !value.is_empty() {
                    template_name = Some(value);
                }
            }
            _ => {}
        }
    }

    let file_data =
        file_data.ok_or_else(|| ApiError::BadRequest("No file provided".to_string()))?;
    let filename =
        filename.ok_or_else(|| ApiError::BadRequest("No filename provided".to_string()))?;

    let file_size_bytes = file_data.len() as u64;

    TemplateParser::validate(&filename, &file_data)?;
    let template = Template::new(template_name.clone(), filename.clone(), file_size_bytes);

    state
        .template_storage
        .put(&template.storage_path, file_data)
        .await?;

    let response = TemplateUploadResponse {
        template_id: template.template_id.to_string(),
        name: template.name.clone(),
        uploaded_at: template.uploaded_at.to_rfc3339(),
        original_filename: template.original_filename,
        file_size_bytes: template.file_size_bytes,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// DELETE /v1/templates/{template_id} — Delete a template object from storage.
/// Object existence is the source of truth; there is no metadata store.
pub async fn delete_template(
    State(state): State<AppState>,
    auth: AuthClaims,
    Path(template_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    auth.require_scope(Scope::TemplatesDelete)?;

    let uuid = uuid::Uuid::parse_str(&template_id)
        .map_err(|_| ApiError::BadRequest("Invalid template_id format".to_string()))?;

    let path = template_storage_path(&uuid);
    let exists =
        state.template_storage.exists(&path).await.map_err(|e| {
            ApiError::Internal(format!("Failed to check template existence: {}", e))
        })?;

    if !exists {
        return Err(ApiError::Template(
            crate::errors::TemplateError::TemplateNotFound(template_id),
        ));
    }

    state.template_storage.delete(&path).await?;
    Ok(StatusCode::NO_CONTENT)
}
