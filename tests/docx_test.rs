use bytes::Bytes;
use rendrr::docx::image_embedder::ImageEmbedder;
use rendrr::services::image_handler::{ImageData, ImageSource, SupportedImageFormat};

// --- ImageSource::parse ---

#[test]
fn image_source_parse_https_url() {
    let source = ImageSource::parse("https://example.com/photo.jpg");
    assert!(matches!(source, ImageSource::Url(url) if url == "https://example.com/photo.jpg"));
}

#[test]
fn image_source_parse_http_url() {
    let source = ImageSource::parse("http://localhost:8080/img.png");
    assert!(matches!(source, ImageSource::Url(url) if url == "http://localhost:8080/img.png"));
}

#[test]
fn image_source_parse_data_url_png() {
    let source = ImageSource::parse("data:image/png;base64,iVBORw0KGgo=");
    assert!(matches!(source, ImageSource::Base64(data) if data == "iVBORw0KGgo="));
}

#[test]
fn image_source_parse_data_url_jpeg() {
    let source = ImageSource::parse("data:image/jpeg;base64,/9j/4AAQ=");
    assert!(matches!(source, ImageSource::Base64(data) if data == "/9j/4AAQ="));
}

#[test]
fn image_source_parse_data_url_without_comma_falls_back_to_url() {
    // data: prefix but no comma — treated as URL
    let source = ImageSource::parse("data:no-comma-here");
    assert!(matches!(source, ImageSource::Url(_)));
}

#[test]
fn image_source_parse_plain_string_is_url() {
    let source = ImageSource::parse("just-a-string");
    assert!(matches!(source, ImageSource::Url(url) if url == "just-a-string"));
}

#[test]
fn image_source_parse_empty_string_is_url() {
    let source = ImageSource::parse("");
    assert!(matches!(source, ImageSource::Url(url) if url.is_empty()));
}

// --- SupportedImageFormat ---

#[test]
fn supported_format_extension_png() {
    assert_eq!(SupportedImageFormat::Png.extension(), "png");
}

#[test]
fn supported_format_extension_jpeg() {
    assert_eq!(SupportedImageFormat::Jpeg.extension(), "jpeg");
}

// --- ImageEmbedder ---

#[test]
fn embedder_add_image_returns_unique_rel_ids() {
    let mut embedder = ImageEmbedder::new();

    let img = ImageData {
        bytes: Bytes::from(vec![0u8; 10]),
        format: SupportedImageFormat::Png,
        width: 100,
        height: 100,
    };

    let id1 = embedder.add_image(img.clone());
    let id2 = embedder.add_image(img.clone());
    let id3 = embedder.add_image(img);

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert!(id1.starts_with("rId"));
    assert!(id2.starts_with("rId"));
    assert!(id3.starts_with("rId"));
}

#[test]
fn embedder_default_same_as_new() {
    let e1 = ImageEmbedder::new();
    let e2 = ImageEmbedder::default();

    let img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 1,
        height: 1,
    };

    // Both should produce the same first rel_id
    let mut e1 = e1;
    let mut e2 = e2;
    assert_eq!(e1.add_image(img.clone()), e2.add_image(img));
}

// --- generate_drawing_xml ---

#[test]
fn drawing_xml_contains_relationship_id() {
    let embedder = ImageEmbedder::new();
    let img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 200,
        height: 100,
    };

    let xml = embedder.generate_drawing_xml("rId42", &img, None, None);
    assert!(xml.contains("rId42"));
    assert!(xml.contains("<w:drawing"));
    assert!(xml.contains("</w:drawing>"));
}

#[test]
fn drawing_xml_scales_down_large_images() {
    let embedder = ImageEmbedder::new();

    // 1000px wide image, no max_width specified — should scale to default 600px
    let large_img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 1000,
        height: 500,
    };

    let xml = embedder.generate_drawing_xml("rId1", &large_img, None, None);

    // 600px * 9525 EMU/px = 5715000
    assert!(xml.contains("5715000"));
}

#[test]
fn drawing_xml_respects_explicit_max_width() {
    let embedder = ImageEmbedder::new();

    let img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 800,
        height: 400,
    };

    let xml = embedder.generate_drawing_xml("rId1", &img, Some(400), None);

    // 400px * 9525 = 3810000
    assert!(xml.contains("3810000"));
}

#[test]
fn drawing_xml_does_not_scale_small_images() {
    let embedder = ImageEmbedder::new();

    let small_img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 100,
        height: 50,
    };

    let xml = embedder.generate_drawing_xml("rId1", &small_img, None, None);

    // 100px * 9525 = 952500
    assert!(xml.contains("952500"));
    // 50px * 9525 = 476250
    assert!(xml.contains("476250"));
}

#[test]
fn drawing_xml_maintains_aspect_ratio_when_scaling() {
    let embedder = ImageEmbedder::new();

    // 800x400 image scaled to max_width=200
    // Expected: 200x100
    let img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 800,
        height: 400,
    };

    let xml = embedder.generate_drawing_xml("rId1", &img, Some(200), None);

    // 200px * 9525 = 1905000 (width)
    assert!(xml.contains("1905000"));
    // 100px * 9525 = 952500 (height)
    assert!(xml.contains("952500"));
}

#[test]
fn drawing_xml_constrains_by_height() {
    let embedder = ImageEmbedder::new();

    // 200x800 image (tall), constrain to height=100
    // Expected: width scales proportionally: 200/800 * 100 = 25, height = 100
    let img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 200,
        height: 800,
    };

    let xml = embedder.generate_drawing_xml("rId1", &img, None, Some(100));

    // 100px * 9525 = 952500 (height)
    assert!(
        xml.contains("952500"),
        "Height not constrained. XML: {}",
        xml
    );
    // 25px * 9525 = 238125 (width, scaled proportionally)
    assert!(
        xml.contains("238125"),
        "Width not scaled proportionally. XML: {}",
        xml
    );
}

#[test]
fn drawing_xml_constrains_by_both_width_and_height() {
    let embedder = ImageEmbedder::new();

    // 1000x2000 image, constrain to width=500 height=200
    // Width constraint first: 500x1000
    // Height constraint then: 1000 > 200, so scale: 200/1000 * 500 = 100, height = 200
    let img = ImageData {
        bytes: Bytes::from(vec![]),
        format: SupportedImageFormat::Png,
        width: 1000,
        height: 2000,
    };

    let xml = embedder.generate_drawing_xml("rId1", &img, Some(500), Some(200));

    // 200px * 9525 = 1905000 (height)
    assert!(
        xml.contains("1905000"),
        "Height not constrained. XML: {}",
        xml
    );
    // 100px * 9525 = 952500 (width)
    assert!(
        xml.contains("952500"),
        "Width not scaled proportionally. XML: {}",
        xml
    );
}

// --- add_image_relationship ---

#[test]
fn add_image_relationship_inserts_before_closing_tag() {
    let embedder = ImageEmbedder::new();
    let rels = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

    let updated = embedder
        .add_image_relationship(rels, "rId100", "image1.png")
        .unwrap();

    assert!(updated.contains("rId100"));
    assert!(updated.contains("media/image1.png"));
    assert!(updated.contains("relationships/image"));
    // The new relationship should appear before </Relationships>
    let rel_pos = updated.find("rId100").unwrap();
    let close_pos = updated.find("</Relationships>").unwrap();
    assert!(rel_pos < close_pos);
}

#[test]
fn add_image_relationship_self_closing_relationships_tag() {
    let embedder = ImageEmbedder::new();
    // Self-closing format as produced by some DOCX editors for empty rels files
    let rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;

    let updated = embedder
        .add_image_relationship(rels, "rId1", "image1.png")
        .unwrap();

    assert!(updated.contains("rId1"));
    assert!(updated.contains("media/image1.png"));
    assert!(updated.contains("</Relationships>"));
}

#[test]
fn add_image_relationship_fails_on_invalid_rels_xml() {
    let embedder = ImageEmbedder::new();
    let bad_xml = "<Not>a valid rels file</Not>";

    let result = embedder.add_image_relationship(bad_xml, "rId1", "img.png");
    assert!(result.is_err());
}

// --- update_relationships ---

#[test]
fn update_relationships_adds_multiple_images() {
    let embedder = ImageEmbedder::new();
    let rels = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

    let image_ids = vec![
        ("rId10".to_string(), "image1.png".to_string()),
        ("rId11".to_string(), "image2.jpeg".to_string()),
    ];

    let updated = embedder.update_relationships(rels, &image_ids).unwrap();

    assert!(updated.contains("rId10"));
    assert!(updated.contains("image1.png"));
    assert!(updated.contains("rId11"));
    assert!(updated.contains("image2.jpeg"));
}

#[test]
fn update_relationships_empty_list_is_noop() {
    let embedder = ImageEmbedder::new();
    let rels = r#"<Relationships xmlns="x">
</Relationships>"#;

    let updated = embedder.update_relationships(rels, &[]).unwrap();
    assert_eq!(updated, rels);
}

// --- ImageHandler (unit tests that don't require network) ---

#[test]
fn image_handler_decode_base64_valid() {
    let handler = rendrr::services::image_handler::ImageHandler::new().unwrap();

    // A minimal valid base64 string (not a real image, but tests decoding)
    let result = handler.decode_base64("SGVsbG8="); // "Hello"
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_ref(), b"Hello");
}

#[test]
fn image_handler_decode_base64_invalid() {
    let handler = rendrr::services::image_handler::ImageHandler::new().unwrap();
    let result = handler.decode_base64("!!!not-base64!!!");
    assert!(result.is_err());
}

#[test]
fn image_handler_default_works() {
    let _handler = rendrr::services::image_handler::ImageHandler::default();
}

#[test]
fn image_handler_validate_invalid_bytes() {
    let handler = rendrr::services::image_handler::ImageHandler::new().unwrap();
    let bad_bytes = Bytes::from(vec![0u8, 1, 2, 3]);
    let result = handler.validate_and_load(&bad_bytes);
    assert!(result.is_err());
}

// Create a minimal valid PNG for testing
fn minimal_png() -> Bytes {
    // Valid 1x1 red pixel PNG (generated with correct CRCs)
    let png_data: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    Bytes::from(png_data)
}

#[test]
fn image_handler_validate_valid_png() {
    let handler = rendrr::services::image_handler::ImageHandler::new().unwrap();
    let png = minimal_png();
    let result = handler.validate_and_load(&png);
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.width, 1);
    assert_eq!(data.height, 1);
    assert_eq!(data.format, SupportedImageFormat::Png);
}

#[test]
fn image_handler_resize_smaller_than_target_returns_original() {
    let handler = rendrr::services::image_handler::ImageHandler::new().unwrap();
    let png = minimal_png();
    let img_data = handler.validate_and_load(&png).unwrap();

    // Image is 1x1, target is 100 — should return as-is
    let resized = handler.resize_image(&img_data, 100).unwrap();
    assert_eq!(resized.width, 1);
    assert_eq!(resized.height, 1);
}
