use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Individual validation error with context and suggestions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    #[serde(rename = "type")]
    pub error_type: String,
    pub expression: String,
    pub suggestion: String,
    pub severity: String, // "error" or "warning"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>, // "syntax" or "data"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Comprehensive validation error report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub status: String, // "validation_failed" or "render_failed"
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn new_syntax(errors: Vec<ValidationIssue>) -> Self {
        let count = errors.len();
        let errors = errors
            .into_iter()
            .map(|mut e| {
                e.category = Some("syntax".to_string());
                e
            })
            .collect();
        Self {
            status: "validation_failed".to_string(),
            message: format!(
                "Template has {} syntax error{}",
                count,
                if count == 1 { "" } else { "s" }
            ),
            errors,
        }
    }

    pub fn new_render(
        syntax_errors: Vec<ValidationIssue>,
        data_errors: Vec<ValidationIssue>,
    ) -> Self {
        let total = syntax_errors.len() + data_errors.len();

        let message = format!(
            "Template has {} error{}",
            total,
            if total == 1 { "" } else { "s" }
        );

        let mut errors: Vec<ValidationIssue> = syntax_errors
            .into_iter()
            .map(|mut e| {
                e.category = Some("syntax".to_string());
                e
            })
            .collect();
        errors.extend(data_errors.into_iter().map(|mut e| {
            e.category = Some("data".to_string());
            e
        }));

        Self {
            status: "render_failed".to_string(),
            message,
            errors,
        }
    }
}

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Invalid DOCX structure: {0}")]
    InvalidDocx(String),

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Template validation failed")]
    ValidationFailed(ValidationReport),

    #[error("File too large: {size} bytes (max {max_size} bytes)")]
    FileTooLarge { size: u64, max_size: u64 },

    #[error("Invalid file type: {0}")]
    InvalidFileType(String),
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("S3 operation failed: {0}")]
    S3Error(#[from] object_store::Error),

    #[error("File not found in storage: {0}")]
    FileNotFound(String),

    #[error("Storage configuration error: {0}")]
    ConfigError(String),

    #[error("Upload failed: {0}")]
    UploadFailed(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),
}

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Template processing error: {0}")]
    TemplateProcessing(String),

    #[error("Data validation error: {0}")]
    DataValidation(String),

    #[error("Rendering failed: {0}")]
    RenderingFailed(String),

    #[error("Image processing error: {0}")]
    ImageProcessing(String),

    #[error("PDF conversion error: {0}")]
    PdfConversion(String),

    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Render error: {0}")]
    Render(#[from] RenderError),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Unauthenticated: {0}")]
    Unauthenticated(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Returns true if verbose error details should be included in API responses.
/// Controlled by `DEBUG_ERRORS=true` env var — only enable in development.
fn debug_errors_enabled() -> bool {
    std::env::var("DEBUG_ERRORS")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

// Implement axum response for ApiError
impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        use axum::Json;
        use serde_json::json;

        match self {
            // Special handling for validation errors - return structured report
            ApiError::Template(TemplateError::ValidationFailed(report)) => {
                let body = Json(report);
                (StatusCode::UNPROCESSABLE_ENTITY, body).into_response()
            }
            ApiError::Render(RenderError::RenderingFailed(msg))
                if msg.starts_with("VALIDATION:") =>
            {
                // Handle render-time validation failures with structured report
                // The msg will be "VALIDATION:{json_report}"
                if let Some(json_str) = msg.strip_prefix("VALIDATION:") {
                    match serde_json::from_str::<ValidationReport>(json_str) {
                        Ok(report) => {
                            let body = Json(report);
                            return (StatusCode::BAD_REQUEST, body).into_response();
                        }
                        Err(e) => {
                            tracing::error!("Failed to deserialize ValidationReport: {}", e);
                            tracing::error!("JSON was: {}", json_str);
                        }
                    }
                }
                let body = Json(json!({
                    "error": "RenderingFailed",
                    "message": "An internal error occurred while rendering the template",
                }));
                (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
            }
            _ => {
                let debug = debug_errors_enabled();

                // Determine status code and safe client-facing error label + message.
                // Client errors (4xx) include the real message; server errors (5xx)
                // get a generic message unless DEBUG_ERRORS is enabled.
                let (status, error_label, client_message) = match &self {
                    ApiError::Template(TemplateError::TemplateNotFound(_)) => {
                        (StatusCode::NOT_FOUND, "TemplateNotFound", self.to_string())
                    }
                    ApiError::Template(_) => {
                        (StatusCode::BAD_REQUEST, "TemplateError", self.to_string())
                    }
                    ApiError::BadRequest(_) => {
                        (StatusCode::BAD_REQUEST, "BadRequest", self.to_string())
                    }
                    ApiError::Unauthenticated(_) => (
                        StatusCode::UNAUTHORIZED,
                        "Unauthenticated",
                        self.to_string(),
                    ),
                    ApiError::PermissionDenied(_) => {
                        (StatusCode::FORBIDDEN, "PermissionDenied", self.to_string())
                    }
                    // Server-side errors — log details, never expose to client
                    ApiError::Storage(_) | ApiError::Render(_) | ApiError::Internal(_) => {
                        tracing::error!("Internal error: {:?}", self);
                        let msg = if debug {
                            self.to_string()
                        } else {
                            "An internal error occurred".to_string()
                        };
                        (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", msg)
                    }
                };

                let body = Json(json!({
                    "error": error_label,
                    "message": client_message,
                }));

                (status, body).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn template_not_found_returns_404() {
        let err = ApiError::Template(TemplateError::TemplateNotFound("abc-123".into()));
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn permission_denied_returns_403() {
        let err =
            ApiError::PermissionDenied("Template does not belong to your organization".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
