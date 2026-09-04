use axum::response::IntoResponse;
use rendrr::errors::*;

// --- ValidationReport ---

#[test]
fn validation_report_new_syntax_single_error() {
    let errors = vec![ValidationIssue {
        error_type: "SyntaxError".to_string(),
        expression: "{{bad".to_string(),
        suggestion: "Close the expression".to_string(),
        severity: "error".to_string(),
        category: None,
        location: Some("word/document.xml".to_string()),
    }];

    let report = ValidationReport::new_syntax(errors);
    assert_eq!(report.status, "validation_failed");
    assert_eq!(report.message, "Template has 1 syntax error");
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].category.as_deref(), Some("syntax"));
}

#[test]
fn validation_report_new_syntax_multiple_errors() {
    let errors = vec![
        ValidationIssue {
            error_type: "SyntaxError".to_string(),
            expression: "{{bad1".to_string(),
            suggestion: "Fix it".to_string(),
            severity: "error".to_string(),
            category: None,
            location: None,
        },
        ValidationIssue {
            error_type: "SyntaxError".to_string(),
            expression: "{{bad2".to_string(),
            suggestion: "Fix it too".to_string(),
            severity: "error".to_string(),
            category: None,
            location: None,
        },
    ];

    let report = ValidationReport::new_syntax(errors);
    assert_eq!(report.message, "Template has 2 syntax errors");
    assert_eq!(report.errors.len(), 2);
}

#[test]
fn validation_report_new_render_both_error_types() {
    let syntax = vec![ValidationIssue {
        error_type: "SyntaxError".to_string(),
        expression: "{{bad".to_string(),
        suggestion: "Fix".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    }];
    let data = vec![ValidationIssue {
        error_type: "MissingField".to_string(),
        expression: "{{name}}".to_string(),
        suggestion: "Provide name".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    }];

    let report = ValidationReport::new_render(syntax, data);
    assert_eq!(report.status, "render_failed");
    assert_eq!(report.message, "Template has 2 errors");
    assert_eq!(report.errors.len(), 2);
    assert_eq!(report.errors[0].category.as_deref(), Some("syntax"));
    assert_eq!(report.errors[1].category.as_deref(), Some("data"));
}

#[test]
fn validation_report_new_render_syntax_only() {
    let syntax = vec![ValidationIssue {
        error_type: "SyntaxError".to_string(),
        expression: "bad".to_string(),
        suggestion: "fix".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    }];

    let report = ValidationReport::new_render(syntax, vec![]);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].category.as_deref(), Some("syntax"));
}

#[test]
fn validation_report_new_render_data_only() {
    let data = vec![ValidationIssue {
        error_type: "MissingField".to_string(),
        expression: "{{x}}".to_string(),
        suggestion: "add x".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    }];

    let report = ValidationReport::new_render(vec![], data);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].category.as_deref(), Some("data"));
}

#[test]
fn validation_report_serialization_skips_empty_errors() {
    let report = ValidationReport::new_syntax(vec![ValidationIssue {
        error_type: "Err".to_string(),
        expression: "e".to_string(),
        suggestion: "s".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    }]);

    let json = serde_json::to_string(&report).unwrap();
    // errors is non-empty, should be present
    assert!(json.contains("\"errors\""));
    // old field names should not appear
    assert!(!json.contains("syntax_errors"));
    assert!(!json.contains("data_errors"));
}

#[test]
fn validation_issue_location_optional() {
    let issue = ValidationIssue {
        error_type: "Err".to_string(),
        expression: "e".to_string(),
        suggestion: "s".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    };

    let json = serde_json::to_string(&issue).unwrap();
    assert!(!json.contains("location"));
    assert!(!json.contains("category"));

    let with_loc = ValidationIssue {
        location: Some("word/document.xml".to_string()),
        ..issue
    };
    let json = serde_json::to_string(&with_loc).unwrap();
    assert!(json.contains("location"));
}

// --- ApiError::into_response status codes ---

#[tokio::test]
async fn api_error_bad_request_returns_400() {
    let err = ApiError::BadRequest("invalid input".to_string());
    let response = err.into_response();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn api_error_template_not_found_returns_404() {
    let err = ApiError::Template(TemplateError::TemplateNotFound("abc123".to_string()));
    let response = err.into_response();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn api_error_invalid_docx_returns_400() {
    let err = ApiError::Template(TemplateError::InvalidDocx("bad zip".to_string()));
    let response = err.into_response();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn api_error_file_too_large_returns_400() {
    let err = ApiError::Template(TemplateError::FileTooLarge {
        size: 20_000_000,
        max_size: 10_000_000,
    });
    let response = err.into_response();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn api_error_invalid_file_type_returns_400() {
    let err = ApiError::Template(TemplateError::InvalidFileType("pdf".to_string()));
    let response = err.into_response();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn api_error_validation_failed_returns_422() {
    let report = ValidationReport::new_syntax(vec![ValidationIssue {
        error_type: "SyntaxError".to_string(),
        expression: "{{bad".to_string(),
        suggestion: "fix".to_string(),
        severity: "error".to_string(),
        category: None,
        location: None,
    }]);
    let err = ApiError::Template(TemplateError::ValidationFailed(report));
    let response = err.into_response();
    assert_eq!(response.status(), 422);
}

#[tokio::test]
async fn api_error_storage_error_returns_500() {
    let err = ApiError::Storage(StorageError::ConfigError("bad config".to_string()));
    let response = err.into_response();
    assert_eq!(response.status(), 500);
}

#[tokio::test]
async fn api_error_internal_returns_500() {
    let err = ApiError::Internal("something broke".to_string());
    let response = err.into_response();
    assert_eq!(response.status(), 500);
}

#[tokio::test]
async fn api_error_render_error_returns_500() {
    let err = ApiError::Render(RenderError::TemplateProcessing("bad template".to_string()));
    let response = err.into_response();
    assert_eq!(response.status(), 500);
}

#[tokio::test]
async fn api_error_render_validation_returns_400_with_structured_report() {
    let report = ValidationReport::new_render(
        vec![],
        vec![ValidationIssue {
            error_type: "MissingField".to_string(),
            expression: "{{name}}".to_string(),
            suggestion: "provide name".to_string(),
            severity: "error".to_string(),
            category: None,
            location: None,
        }],
    );
    let report_json = serde_json::to_string(&report).unwrap();
    let err = ApiError::Render(RenderError::RenderingFailed(format!(
        "VALIDATION:{}",
        report_json
    )));
    let response = err.into_response();
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn api_error_render_validation_malformed_json_returns_500() {
    let err = ApiError::Render(RenderError::RenderingFailed(
        "VALIDATION:{not valid json}".to_string(),
    ));
    let response = err.into_response();
    assert_eq!(response.status(), 500);
}

// --- Error Display ---

#[test]
fn template_error_display() {
    let err = TemplateError::TemplateNotFound("abc".to_string());
    assert_eq!(err.to_string(), "Template not found: abc");

    let err = TemplateError::FileTooLarge {
        size: 20,
        max_size: 10,
    };
    assert!(err.to_string().contains("20"));
    assert!(err.to_string().contains("10"));
}

#[test]
fn storage_error_display() {
    let err = StorageError::FileNotFound("renders/abc.docx".to_string());
    assert!(err.to_string().contains("renders/abc.docx"));

    let err = StorageError::UploadFailed("timeout".to_string());
    assert!(err.to_string().contains("timeout"));

    let err = StorageError::DownloadFailed("connection reset".to_string());
    assert!(err.to_string().contains("connection reset"));
}

#[test]
fn render_error_display() {
    let err = RenderError::TemplateProcessing("bad xml".to_string());
    assert!(err.to_string().contains("bad xml"));

    let err = RenderError::DataValidation("too deep".to_string());
    assert!(err.to_string().contains("too deep"));

    let err = RenderError::ImageProcessing("404".to_string());
    assert!(err.to_string().contains("404"));
}

// --- Error conversions ---

#[test]
fn storage_error_from_template_error() {
    let template_err = TemplateError::TemplateNotFound("x".to_string());
    let api_err: ApiError = template_err.into();
    assert!(matches!(api_err, ApiError::Template(_)));
}

#[test]
fn storage_error_into_api_error() {
    let storage_err = StorageError::ConfigError("bad".to_string());
    let api_err: ApiError = storage_err.into();
    assert!(matches!(api_err, ApiError::Storage(_)));
}

#[test]
fn render_error_into_api_error() {
    let render_err = RenderError::RenderingFailed("fail".to_string());
    let api_err: ApiError = render_err.into();
    assert!(matches!(api_err, ApiError::Render(_)));
}

#[test]
fn storage_error_into_render_error() {
    let storage_err = StorageError::FileNotFound("x".to_string());
    let render_err: RenderError = storage_err.into();
    assert!(matches!(render_err, RenderError::Storage(_)));
}

#[test]
fn template_error_into_render_error() {
    let template_err = TemplateError::InvalidDocx("bad".to_string());
    let render_err: RenderError = template_err.into();
    assert!(matches!(render_err, RenderError::Template(_)));
}
