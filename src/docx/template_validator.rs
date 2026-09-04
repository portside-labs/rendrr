//! DOCX-specific structural and syntactic validation for uploaded templates.

use crate::docx::DocxTemplateEngine;
use crate::errors::TemplateError;
use bytes::Bytes;
use std::io::Cursor;
use zip::ZipArchive;

/// Validate that `data` is a well-formed DOCX template whose Handlebars
/// expressions parse cleanly. Caller is responsible for extension and size
/// checks.
pub fn validate(data: &Bytes) -> Result<(), TemplateError> {
    validate_structure(data)?;
    DocxTemplateEngine::new()
        .validate_template_syntax(data)
        .map_err(TemplateError::ValidationFailed)?;
    Ok(())
}

fn validate_structure(data: &Bytes) -> Result<(), TemplateError> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| TemplateError::InvalidDocx(format!("Not a valid ZIP archive: {}", e)))?;

    for required in &["[Content_Types].xml", "word/document.xml"] {
        if archive.by_name(required).is_err() {
            return Err(TemplateError::InvalidDocx(format!(
                "Missing required file: {}",
                required
            )));
        }
    }
    Ok(())
}
