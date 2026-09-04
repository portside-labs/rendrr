// DOCX image embedding logic
// Emits the Word `<w:drawing>` XML for an embedded image and tracks the
// relationship IDs that `word/_rels/document.xml.rels` needs.

use crate::errors::RenderError;
use crate::services::image_handler::ImageData;
use std::collections::HashMap;

/// Image embedding context for a DOCX document
pub struct ImageEmbedder {
    /// Counter for generating unique relationship IDs
    next_rel_id: u32,
    /// Counter for generating unique image file names
    next_image_id: u32,
    /// Map of image ID to image data
    images: HashMap<String, ImageData>,
}

impl ImageEmbedder {
    pub fn new() -> Self {
        Self {
            next_rel_id: 1000, // Start at 1000 to avoid conflicts with existing rels (headers, footers, etc.)
            next_image_id: 1,
            images: HashMap::new(),
        }
    }

    /// Add an image and return its relationship ID
    pub fn add_image(&mut self, image_data: ImageData) -> String {
        let rel_id = format!("rId{}", self.next_rel_id);
        let image_id = format!("image{}", self.next_image_id);

        self.next_rel_id += 1;
        self.next_image_id += 1;

        self.images.insert(image_id.clone(), image_data);

        rel_id
    }

    /// Generate Word drawing XML for an inline image.
    /// This creates the XML structure that Word uses to display images
    pub fn generate_drawing_xml(
        &self,
        rel_id: &str,
        image_data: &ImageData,
        max_width_px: Option<u32>,
        max_height_px: Option<u32>,
    ) -> String {
        // Calculate dimensions in EMUs (English Metric Units)
        // 1 inch = 914400 EMUs, 1 pixel ≈ 9525 EMUs (at 96 DPI)
        const EMUS_PER_PIXEL: u32 = 9525;
        const DEFAULT_MAX_WIDTH_PX: u32 = 600; // Default to 600px (about 6.25 inches at 96 DPI)

        // Use provided max width, or default to 600px if image is larger
        let effective_max_width = max_width_px.or(if image_data.width > DEFAULT_MAX_WIDTH_PX {
            Some(DEFAULT_MAX_WIDTH_PX)
        } else {
            None
        });

        // First constrain by width (maintaining aspect ratio)
        let (mut width_px, mut height_px) = if let Some(max_w) = effective_max_width {
            if image_data.width > max_w {
                let aspect_ratio = image_data.height as f64 / image_data.width as f64;
                (max_w, (max_w as f64 * aspect_ratio) as u32)
            } else {
                (image_data.width, image_data.height)
            }
        } else {
            (image_data.width, image_data.height)
        };

        // Then constrain by height if specified (maintaining aspect ratio)
        if let Some(max_h) = max_height_px {
            if height_px > max_h {
                let aspect_ratio = width_px as f64 / height_px as f64;
                height_px = max_h;
                width_px = (max_h as f64 * aspect_ratio) as u32;
            }
        }

        let width_emus = width_px * EMUS_PER_PIXEL;
        let height_emus = height_px * EMUS_PER_PIXEL;

        format!(
            r#"<w:drawing xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
    <wp:inline distT="0" distB="0" distL="0" distR="0">
        <wp:extent cx="{width_emus}" cy="{height_emus}"/>
        <wp:effectExtent l="0" t="0" r="0" b="0"/>
        <wp:docPr id="1" name="Picture"/>
        <wp:cNvGraphicFramePr>
            <a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/>
        </wp:cNvGraphicFramePr>
        <a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
            <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
                <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
                    <pic:nvPicPr>
                        <pic:cNvPr id="0" name="Picture"/>
                        <pic:cNvPicPr/>
                    </pic:nvPicPr>
                    <pic:blipFill>
                        <a:blip r:embed="{rel_id}"/>
                        <a:stretch>
                            <a:fillRect/>
                        </a:stretch>
                    </pic:blipFill>
                    <pic:spPr>
                        <a:xfrm>
                            <a:off x="0" y="0"/>
                            <a:ext cx="{width_emus}" cy="{height_emus}"/>
                        </a:xfrm>
                        <a:prstGeom prst="rect">
                            <a:avLst/>
                        </a:prstGeom>
                    </pic:spPr>
                </pic:pic>
            </a:graphicData>
        </a:graphic>
    </wp:inline>
</w:drawing>"#,
            width_emus = width_emus,
            height_emus = height_emus,
            rel_id = rel_id,
        )
    }

    /// Add a single image relationship entry to a `.rels` document.
    /// Thin wrapper around the shared OOXML rels helper.
    pub fn add_image_relationship(
        &self,
        rels_xml: &str,
        rel_id: &str,
        image_filename: &str,
    ) -> Result<String, RenderError> {
        crate::services::ooxml_rels::add_image_relationship(rels_xml, rel_id, image_filename)
    }

    /// Update relationships to include image references.
    pub fn update_relationships(
        &self,
        rels_xml: &str,
        image_ids: &[(String, String)],
    ) -> Result<String, RenderError> {
        crate::services::ooxml_rels::update_relationships(rels_xml, image_ids)
    }
}

impl Default for ImageEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::image_handler::SupportedImageFormat;
    use bytes::Bytes;

    #[test]
    fn test_generate_drawing_xml() {
        let embedder = ImageEmbedder::new();

        let image_data = ImageData {
            bytes: Bytes::from(vec![]),
            format: SupportedImageFormat::Png,
            width: 800,
            height: 600,
        };

        let drawing_xml = embedder.generate_drawing_xml("rId100", &image_data, None, None);

        println!("Generated XML:\n{}", drawing_xml);

        assert!(drawing_xml.contains("rId100"), "Should contain rId100");
        assert!(
            drawing_xml.contains("<w:drawing"),
            "Should contain <w:drawing tag"
        );
        assert!(
            drawing_xml.contains("</w:drawing>"),
            "Should contain </w:drawing> closing tag"
        );
    }

    #[test]
    fn test_add_image_relationship() {
        let embedder = ImageEmbedder::new();

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

        let updated = embedder
            .add_image_relationship(rels_xml, "rId100", "image1.png")
            .unwrap();

        assert!(updated.contains("rId100"));
        assert!(updated.contains("image1.png"));
        assert!(updated.contains("relationships/image"));
    }

    #[test]
    fn test_add_image_relationship_self_closing_tag() {
        let embedder = ImageEmbedder::new();

        // Self-closing <Relationships/> as produced by some DOCX editors for empty rels files
        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;

        let updated = embedder
            .add_image_relationship(rels_xml, "rId1", "image1.png")
            .unwrap();

        assert!(updated.contains("rId1"));
        assert!(updated.contains("image1.png"));
        assert!(updated.contains("relationships/image"));
        assert!(
            updated.contains("</Relationships>"),
            "Should convert to non-self-closing tag"
        );
        assert!(
            !updated.ends_with("/>"),
            "Should not end with self-closing tag"
        );
    }
}
