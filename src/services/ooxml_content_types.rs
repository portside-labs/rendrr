//! Shared `[Content_Types].xml` patching. The `<Default>` entries declaring
//! PNG/JPEG content types are declared the same way in any OOXML package.

use crate::errors::RenderError;

/// Ensure `<Default>` declarations for PNG and JPEG are present in the
/// `[Content_Types].xml` of an OOXML package. Inserts the declarations
/// before the closing `</Types>` tag if missing.
pub fn ensure_image_content_types(content_types_xml: &str) -> Result<String, RenderError> {
    let mut result = content_types_xml.to_string();

    if !result.contains("Extension=\"png\"") && !result.contains("Extension='png'") {
        let closing_pos = result.rfind("</Types>").ok_or_else(|| {
            RenderError::TemplateProcessing("Invalid [Content_Types].xml format".to_string())
        })?;
        let png = r#"<Default Extension="png" ContentType="image/png"/>"#;
        result.insert_str(closing_pos, png);
        result.insert(closing_pos + png.len(), '\n');
    }

    if !result.contains("Extension=\"jpeg\"") && !result.contains("Extension='jpeg'") {
        let closing_pos = result.rfind("</Types>").ok_or_else(|| {
            RenderError::TemplateProcessing("Invalid [Content_Types].xml format".to_string())
        })?;
        let jpeg = r#"<Default Extension="jpeg" ContentType="image/jpeg"/>"#;
        result.insert_str(closing_pos, jpeg);
        result.insert(closing_pos + jpeg.len(), '\n');
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_png_and_jpeg_when_absent() {
        let ct = r#"<?xml version="1.0"?><Types xmlns="x"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>"#;
        let updated = ensure_image_content_types(ct).unwrap();
        assert!(updated.contains(r#"Extension="png""#));
        assert!(updated.contains(r#"Extension="jpeg""#));
    }

    #[test]
    fn does_not_duplicate_existing_declarations() {
        let ct = r#"<?xml version="1.0"?><Types xmlns="x"><Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/></Types>"#;
        let updated = ensure_image_content_types(ct).unwrap();
        // Count occurrences — should still be exactly one of each
        assert_eq!(updated.matches(r#"Extension="png""#).count(), 1);
        assert_eq!(updated.matches(r#"Extension="jpeg""#).count(), 1);
    }

    #[test]
    fn errors_on_missing_closing_tag() {
        let result = ensure_image_content_types("<Types xmlns=\"x\">");
        assert!(result.is_err());
    }
}
