//! Fetching, validating, and resizing images referenced by the `{{image}}`
//! helper. Sources are either an `http(s)` URL or an inline base64 data URL.
//!
//! URLs come from caller-supplied render data, so every outbound request is
//! screened by [`crate::services::url_guard`] first and the response body is
//! capped while it streams — see [`ImageHandler::fetch_from_url`].

use crate::errors::RenderError;
use crate::services::url_guard;
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use image::{imageops::FilterType, DynamicImage, GenericImageView, ImageFormat};
use std::io::Cursor;

/// Maximum image size in bytes (10MB)
const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

/// Maximum image dimensions (width or height)
const MAX_IMAGE_DIMENSION: u32 = 4096;

/// HTTP request timeout in seconds
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Maximum number of redirects followed for an image URL. Redirects are
/// followed by hand so every hop is re-screened by the SSRF guard.
const MAX_REDIRECTS: usize = 3;

/// Supported image formats for DOCX
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedImageFormat {
    Png,
    Jpeg,
}

impl SupportedImageFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &str {
        match self {
            SupportedImageFormat::Png => "png",
            SupportedImageFormat::Jpeg => "jpeg",
        }
    }
}

/// Image data with metadata
#[derive(Debug, Clone)]
pub struct ImageData {
    pub bytes: Bytes,
    pub format: SupportedImageFormat,
    pub width: u32,
    pub height: u32,
}

/// Image source - either URL or base64-encoded data
#[derive(Debug, Clone)]
pub enum ImageSource {
    Url(String),
    Base64(String),
}

impl ImageSource {
    /// Parse an image source from a string
    /// Detects base64 data URLs (data:image/png;base64,...)
    pub fn parse(input: &str) -> Self {
        if input.starts_with("data:") {
            // Extract base64 data from data URL
            if let Some(comma_pos) = input.find(',') {
                let base64_data = &input[comma_pos + 1..];
                return ImageSource::Base64(base64_data.to_string());
            }
        }

        // Default to URL
        ImageSource::Url(input.to_string())
    }
}

/// Image handler for fetching, validating, and processing images
pub struct ImageHandler {
    client: reqwest::Client,
}

impl ImageHandler {
    /// Create a new ImageHandler.
    ///
    /// Automatic redirect following is disabled: each hop has to be re-checked
    /// against the SSRF guard, which `reqwest`'s redirect policy can't do
    /// (the policy callback is synchronous and can't resolve DNS).
    pub fn new() -> Result<Self, RenderError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| {
                RenderError::ImageProcessing(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client })
    }

    /// Fetch an image over HTTP(S).
    ///
    /// Every hop — the original URL and each redirect target — is screened by
    /// [`url_guard`] before a connection is made, and the response body is
    /// capped while it streams rather than after it lands, so a hostile server
    /// advertising a small image and then sending gigabytes can't exhaust
    /// memory.
    pub async fn fetch_from_url(&self, url: &str) -> Result<Bytes, RenderError> {
        let mut current = url_guard::validate_scheme(url)?;

        for _ in 0..=MAX_REDIRECTS {
            url_guard::assert_host_is_public(&current).await?;

            let response = self.client.get(current.clone()).send().await.map_err(|e| {
                if e.is_timeout() {
                    RenderError::ImageProcessing(format!(
                        "Image download timeout after {}s: {}",
                        REQUEST_TIMEOUT_SECS, current
                    ))
                } else if e.is_connect() {
                    RenderError::ImageProcessing(format!(
                        "Failed to connect to image URL: {}",
                        current
                    ))
                } else {
                    RenderError::ImageProcessing(format!(
                        "Failed to download image from {}: {}",
                        current, e
                    ))
                }
            })?;

            let status = response.status();

            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        RenderError::ImageProcessing(format!(
                            "Image URL {} returned {} without a Location header",
                            current, status
                        ))
                    })?;

                // Resolve relative redirects against the URL we just fetched,
                // then re-validate the scheme so a redirect can't downgrade to
                // something like file:// or gopher://.
                let next = current.join(location).map_err(|e| {
                    RenderError::ImageProcessing(format!(
                        "Invalid redirect target {:?} from {}: {}",
                        location, current, e
                    ))
                })?;
                current = url_guard::validate_scheme(next.as_str())?;
                continue;
            }

            if !status.is_success() {
                return Err(RenderError::ImageProcessing(format!(
                    "Image download failed with status {}: {}",
                    status, current
                )));
            }

            return read_body_capped(response, &current).await;
        }

        Err(RenderError::ImageProcessing(format!(
            "Image URL {} exceeded the redirect limit of {}",
            url, MAX_REDIRECTS
        )))
    }

    /// Decode base64-encoded image data.
    pub fn decode_base64(&self, base64_data: &str) -> Result<Bytes, RenderError> {
        let decoded = general_purpose::STANDARD.decode(base64_data).map_err(|e| {
            RenderError::ImageProcessing(format!("Failed to decode base64 image: {}", e))
        })?;

        if decoded.len() > MAX_IMAGE_SIZE {
            return Err(RenderError::ImageProcessing(format!(
                "Image size {} bytes exceeds maximum of {} bytes",
                decoded.len(),
                MAX_IMAGE_SIZE
            )));
        }

        Ok(Bytes::from(decoded))
    }

    /// Validate image format and extract metadata.
    pub fn validate_and_load(&self, bytes: &Bytes) -> Result<ImageData, RenderError> {
        // Try to load as image
        let img = image::load_from_memory(bytes)
            .map_err(|e| RenderError::ImageProcessing(format!("Invalid image format: {}", e)))?;

        // Detect format
        let format = image::guess_format(bytes).map_err(|e| {
            RenderError::ImageProcessing(format!("Could not detect image format: {}", e))
        })?;

        // Map to supported format
        let supported_format = match format {
            ImageFormat::Png => SupportedImageFormat::Png,
            ImageFormat::Jpeg => SupportedImageFormat::Jpeg,
            _ => {
                // Convert unsupported formats to PNG
                return self.convert_to_png(img);
            }
        };

        let (width, height) = img.dimensions();

        if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
            return Err(RenderError::ImageProcessing(format!(
                "Image dimensions {}x{} exceed maximum of {}",
                width, height, MAX_IMAGE_DIMENSION
            )));
        }

        Ok(ImageData {
            bytes: bytes.clone(),
            format: supported_format,
            width,
            height,
        })
    }

    /// Convert image to PNG format
    fn convert_to_png(&self, img: DynamicImage) -> Result<ImageData, RenderError> {
        let (width, height) = img.dimensions();

        let mut buffer = Vec::new();
        img.write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)
            .map_err(|e| {
                RenderError::ImageProcessing(format!("Failed to convert image to PNG: {}", e))
            })?;

        Ok(ImageData {
            bytes: Bytes::from(buffer),
            format: SupportedImageFormat::Png,
            width,
            height,
        })
    }

    /// Resize an image, maintaining aspect ratio.
    /// target_width: desired width in pixels (height will be calculated proportionally)
    pub fn resize_image(
        &self,
        img_data: &ImageData,
        target_width: u32,
    ) -> Result<ImageData, RenderError> {
        // If already smaller than target, return as-is
        if img_data.width <= target_width {
            return Ok(ImageData {
                bytes: img_data.bytes.clone(),
                format: img_data.format,
                width: img_data.width,
                height: img_data.height,
            });
        }

        // Load image
        let img = image::load_from_memory(&img_data.bytes).map_err(|e| {
            RenderError::ImageProcessing(format!("Failed to load image for resizing: {}", e))
        })?;

        // Calculate new dimensions maintaining aspect ratio
        let aspect_ratio = img_data.height as f64 / img_data.width as f64;
        let new_height = (target_width as f64 * aspect_ratio) as u32;

        // Resize
        let resized = img.resize(target_width, new_height, FilterType::Lanczos3);

        // Encode to original format
        let mut buffer = Vec::new();
        let image_format = match img_data.format {
            SupportedImageFormat::Png => ImageFormat::Png,
            SupportedImageFormat::Jpeg => ImageFormat::Jpeg,
        };

        resized
            .write_to(&mut Cursor::new(&mut buffer), image_format)
            .map_err(|e| {
                RenderError::ImageProcessing(format!("Failed to encode resized image: {}", e))
            })?;

        Ok(ImageData {
            bytes: Bytes::from(buffer),
            format: img_data.format,
            width: target_width,
            height: new_height,
        })
    }

    /// Main entry point: fetch and process an image from any source
    pub async fn fetch_and_process(
        &self,
        source: &ImageSource,
        max_width: Option<u32>,
    ) -> Result<ImageData, RenderError> {
        // Fetch image bytes
        let bytes = match source {
            ImageSource::Url(url) => self.fetch_from_url(url).await?,
            ImageSource::Base64(data) => self.decode_base64(data)?,
        };

        // Validate and load
        let mut img_data = self.validate_and_load(&bytes)?;

        // Resize if requested
        if let Some(target_width) = max_width {
            img_data = self.resize_image(&img_data, target_width)?;
        }

        Ok(img_data)
    }
}

/// Read a response body, aborting as soon as it exceeds [`MAX_IMAGE_SIZE`].
///
/// `Response::bytes()` would buffer the whole body first and only then let us
/// check the length, which turns a hostile URL into a memory-exhaustion vector.
/// `Content-Length`, when present, lets us reject before reading anything —
/// but it is attacker-controlled, so the streaming check still has to be there.
async fn read_body_capped(
    mut response: reqwest::Response,
    url: &reqwest::Url,
) -> Result<Bytes, RenderError> {
    if let Some(len) = response.content_length() {
        if len > MAX_IMAGE_SIZE as u64 {
            return Err(RenderError::ImageProcessing(format!(
                "Image size {} bytes exceeds maximum of {} bytes: {}",
                len, MAX_IMAGE_SIZE, url
            )));
        }
    }

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        RenderError::ImageProcessing(format!("Failed to read image data from {}: {}", url, e))
    })? {
        if buf.len() + chunk.len() > MAX_IMAGE_SIZE {
            return Err(RenderError::ImageProcessing(format!(
                "Image exceeds maximum of {} bytes: {}",
                MAX_IMAGE_SIZE, url
            )));
        }
        buf.extend_from_slice(&chunk);
    }

    Ok(Bytes::from(buf))
}

impl Default for ImageHandler {
    fn default() -> Self {
        Self::new().expect("Failed to create ImageHandler")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_source_parse_url() {
        let source = ImageSource::parse("https://example.com/image.png");
        match source {
            ImageSource::Url(url) => assert_eq!(url, "https://example.com/image.png"),
            _ => panic!("Expected URL source"),
        }
    }

    #[test]
    fn test_image_source_parse_base64() {
        let source = ImageSource::parse("data:image/png;base64,iVBORw0KGgo=");
        match source {
            ImageSource::Base64(data) => assert_eq!(data, "iVBORw0KGgo="),
            _ => panic!("Expected Base64 source"),
        }
    }

    #[test]
    fn test_supported_format_extension() {
        assert_eq!(SupportedImageFormat::Png.extension(), "png");
        assert_eq!(SupportedImageFormat::Jpeg.extension(), "jpeg");
    }

    #[test]
    fn parse_data_url_with_no_comma_falls_back_to_url() {
        let source = ImageSource::parse("data:image/png;base64");
        match source {
            ImageSource::Url(s) => assert_eq!(s, "data:image/png;base64"),
            _ => panic!("expected URL fallback"),
        }
    }

    fn make_png(width: u32, height: u32) -> Bytes {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        Bytes::from(buf)
    }

    fn make_jpeg(width: u32, height: u32) -> Bytes {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([0, 128, 255]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
            .unwrap();
        Bytes::from(buf)
    }

    fn make_gif(width: u32, height: u32) -> Bytes {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([0, 255, 0, 255]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Gif)
            .unwrap();
        Bytes::from(buf)
    }

    #[test]
    fn new_returns_handler() {
        ImageHandler::new().unwrap();
    }

    #[test]
    fn default_handler_constructs() {
        let _ = ImageHandler::default();
    }

    #[test]
    fn decode_base64_valid_round_trips() {
        let handler = ImageHandler::new().unwrap();
        let png = make_png(2, 2);
        let encoded = general_purpose::STANDARD.encode(&png);
        let decoded = handler.decode_base64(&encoded).unwrap();
        assert_eq!(decoded.as_ref(), png.as_ref());
    }

    #[test]
    fn decode_base64_invalid_returns_error() {
        let handler = ImageHandler::new().unwrap();
        let err = handler.decode_base64("!!!not base64").unwrap_err();
        assert!(matches!(err, RenderError::ImageProcessing(_)));
    }

    #[test]
    fn decode_base64_rejects_oversized_payload() {
        let handler = ImageHandler::new().unwrap();
        // Encode 11 MB of zeros → over the 10 MB limit.
        let big = vec![0u8; 11 * 1024 * 1024];
        let encoded = general_purpose::STANDARD.encode(&big);
        let err = handler.decode_base64(&encoded).unwrap_err();
        assert!(matches!(err, RenderError::ImageProcessing(_)));
    }

    #[test]
    fn validate_and_load_accepts_png() {
        let handler = ImageHandler::new().unwrap();
        let png = make_png(10, 20);
        let data = handler.validate_and_load(&png).unwrap();
        assert_eq!(data.format, SupportedImageFormat::Png);
        assert_eq!(data.width, 10);
        assert_eq!(data.height, 20);
    }

    #[test]
    fn validate_and_load_accepts_jpeg() {
        let handler = ImageHandler::new().unwrap();
        let jpeg = make_jpeg(8, 8);
        let data = handler.validate_and_load(&jpeg).unwrap();
        assert_eq!(data.format, SupportedImageFormat::Jpeg);
    }

    #[test]
    fn validate_and_load_converts_unsupported_to_png() {
        let handler = ImageHandler::new().unwrap();
        let gif = make_gif(4, 4);
        let data = handler.validate_and_load(&gif).unwrap();
        assert_eq!(data.format, SupportedImageFormat::Png);
    }

    #[test]
    fn validate_and_load_rejects_garbage_bytes() {
        let handler = ImageHandler::new().unwrap();
        let err = handler
            .validate_and_load(&Bytes::from_static(b"not an image"))
            .unwrap_err();
        assert!(matches!(err, RenderError::ImageProcessing(_)));
    }

    #[test]
    fn validate_and_load_rejects_oversized_dimensions() {
        let handler = ImageHandler::new().unwrap();
        // 5000×5000 is over MAX_IMAGE_DIMENSION = 4096.
        let big = make_png(5000, 10);
        let err = handler.validate_and_load(&big).unwrap_err();
        assert!(matches!(err, RenderError::ImageProcessing(_)));
    }

    #[test]
    fn resize_image_downsizes_png() {
        let handler = ImageHandler::new().unwrap();
        let big = make_png(200, 100);
        let img_data = handler.validate_and_load(&big).unwrap();
        let resized = handler.resize_image(&img_data, 50).unwrap();
        assert_eq!(resized.width, 50);
        assert_eq!(resized.height, 25);
        assert_eq!(resized.format, SupportedImageFormat::Png);
    }

    #[test]
    fn resize_image_downsizes_jpeg() {
        let handler = ImageHandler::new().unwrap();
        let big = make_jpeg(200, 50);
        let img_data = handler.validate_and_load(&big).unwrap();
        let resized = handler.resize_image(&img_data, 100).unwrap();
        assert_eq!(resized.width, 100);
        assert_eq!(resized.format, SupportedImageFormat::Jpeg);
    }

    #[test]
    fn resize_image_no_op_when_already_small() {
        let handler = ImageHandler::new().unwrap();
        let small = make_png(20, 10);
        let img_data = handler.validate_and_load(&small).unwrap();
        let resized = handler.resize_image(&img_data, 100).unwrap();
        assert_eq!(resized.width, 20);
        assert_eq!(resized.height, 10);
    }

    #[tokio::test]
    async fn fetch_and_process_base64_succeeds() {
        let handler = ImageHandler::new().unwrap();
        let png = make_png(16, 16);
        let encoded = general_purpose::STANDARD.encode(&png);
        let data = handler
            .fetch_and_process(&ImageSource::Base64(encoded), None)
            .await
            .unwrap();
        assert_eq!(data.width, 16);
    }

    #[tokio::test]
    async fn fetch_and_process_base64_with_resize() {
        let handler = ImageHandler::new().unwrap();
        let png = make_png(100, 100);
        let encoded = general_purpose::STANDARD.encode(&png);
        let data = handler
            .fetch_and_process(&ImageSource::Base64(encoded), Some(25))
            .await
            .unwrap();
        assert_eq!(data.width, 25);
    }

    #[tokio::test]
    async fn fetch_from_url_unreachable_returns_error() {
        let handler = ImageHandler::new().unwrap();
        // Use a discard port that will refuse the connection.
        let err = handler
            .fetch_from_url("http://127.0.0.1:1/never-listening")
            .await
            .unwrap_err();
        assert!(matches!(err, RenderError::ImageProcessing(_)));
    }
}
