//! Shared OOXML relationship-XML manipulation. Relationship
//! files (`.rels`) share the same XML schema, so the functions here work for
//! either format.

use crate::errors::RenderError;

/// Add a single image relationship entry to a `.rels` XML document. Handles
/// both closed (`</Relationships>`) and self-closing (`<Relationships .../>`)
/// root forms.
pub fn add_image_relationship(
    rels_xml: &str,
    rel_id: &str,
    image_filename: &str,
) -> Result<String, RenderError> {
    let relationship = format!(
        r#"<Relationship Id="{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/{}"/>"#,
        rel_id, image_filename
    );

    if let Some(closing_pos) = rels_xml.rfind("</Relationships>") {
        let mut result = String::new();
        result.push_str(&rels_xml[..closing_pos]);
        result.push_str(&relationship);
        result.push('\n');
        result.push_str(&rels_xml[closing_pos..]);
        return Ok(result);
    }

    if let Some(pos) = rels_xml.rfind("/>") {
        let before = &rels_xml[..pos];
        if before.contains("<Relationships") {
            let mut result = String::new();
            result.push_str(&rels_xml[..pos]);
            result.push_str(">\n");
            result.push_str(&relationship);
            result.push_str("\n</Relationships>");
            return Ok(result);
        }
    }

    Err(RenderError::TemplateProcessing(
        "Invalid .rels file format: no Relationships tag found".to_string(),
    ))
}

/// Add multiple image relationship entries. `image_ids` is a slice of
/// `(rel_id, image_filename)` pairs.
pub fn update_relationships(
    rels_xml: &str,
    image_ids: &[(String, String)],
) -> Result<String, RenderError> {
    let mut result = rels_xml.to_string();
    for (rel_id, image_filename) in image_ids {
        result = add_image_relationship(&result, rel_id, image_filename)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_image_relationship_appends_before_closing_tag() {
        let rels = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="x" Target="styles.xml"/>
</Relationships>"#;
        let updated = add_image_relationship(rels, "rId100", "image1.png").unwrap();
        assert!(updated.contains("rId100"));
        assert!(updated.contains("image1.png"));
        assert!(updated.contains("relationships/image"));
    }

    #[test]
    fn add_image_relationship_handles_self_closing_root() {
        let rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
        let updated = add_image_relationship(rels, "rId1", "image1.png").unwrap();
        assert!(updated.contains("</Relationships>"));
        assert!(!updated.ends_with("/>"));
        assert!(updated.contains("rId1"));
    }

    #[test]
    fn update_relationships_appends_all() {
        let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;
        let pairs = vec![
            ("rId10".to_string(), "img1.png".to_string()),
            ("rId11".to_string(), "img2.jpg".to_string()),
        ];
        let updated = update_relationships(rels, &pairs).unwrap();
        assert!(updated.contains("rId10"));
        assert!(updated.contains("rId11"));
        assert!(updated.contains("img1.png"));
        assert!(updated.contains("img2.jpg"));
    }
}
