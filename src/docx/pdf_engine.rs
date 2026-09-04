use bytes::Bytes;

use crate::errors::RenderError;

/// In-process DOCX → PDF converter backed by the `dxpdf` crate (Skia-based).
///
/// No external services or processes — the conversion happens inside this binary.
#[derive(Clone, Debug)]
pub struct PdfEngine;

impl PdfEngine {
    pub fn new() -> Self {
        Self
    }

    /// PDF rendering is on by default. The `PDF_ENGINE` env var is kept as an
    /// extension point: it accepts `"dxpdf"` (the default, the only engine that
    /// currently exists) or `"none"` (disables PDF entirely). Unknown values
    /// warn and fall back to dxpdf. Most operators should leave it unset.
    pub fn from_env() -> Option<Self> {
        match std::env::var("PDF_ENGINE").ok().as_deref() {
            Some("none") => None,
            None | Some("") | Some("dxpdf") => Some(Self::new()),
            Some(other) => {
                tracing::warn!(
                    "Unknown PDF_ENGINE value {:?}; falling back to the default engine.",
                    other
                );
                Some(Self::new())
            }
        }
    }

    pub async fn convert_docx_to_pdf(&self, docx_bytes: Bytes) -> Result<Bytes, RenderError> {
        let bytes = docx_bytes.to_vec();
        let pdf = tokio::task::spawn_blocking(move || dxpdf::convert(&bytes))
            .await
            .map_err(|e| RenderError::PdfConversion(format!("PDF task panicked: {}", e)))?
            .map_err(|e| RenderError::PdfConversion(format!("dxpdf failed: {}", e)))?;
        Ok(Bytes::from(pdf))
    }
}

impl Default for PdfEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests that flip env vars can race when run in parallel. We use a serial
    // mutex via `serial_test`-style manual locking on a static Mutex.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_env<F: FnOnce()>(key: &str, value: Option<&str>, f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(key).ok();
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        f();
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn default_constructs_a_new_engine() {
        #[allow(clippy::default_constructed_unit_structs)]
        let _ = PdfEngine::default();
    }

    #[test]
    fn from_env_unset_defaults_to_engine_enabled() {
        with_env("PDF_ENGINE", None, || {
            assert!(PdfEngine::from_env().is_some());
        });
    }

    #[test]
    fn from_env_empty_string_defaults_to_engine_enabled() {
        with_env("PDF_ENGINE", Some(""), || {
            assert!(PdfEngine::from_env().is_some());
        });
    }

    #[test]
    fn from_env_dxpdf_returns_engine() {
        with_env("PDF_ENGINE", Some("dxpdf"), || {
            assert!(PdfEngine::from_env().is_some());
        });
    }

    #[test]
    fn from_env_none_disables_engine() {
        with_env("PDF_ENGINE", Some("none"), || {
            assert!(PdfEngine::from_env().is_none());
        });
    }

    #[test]
    fn from_env_unknown_value_falls_back_to_default() {
        with_env("PDF_ENGINE", Some("ghostscript"), || {
            assert!(PdfEngine::from_env().is_some());
        });
    }

    #[tokio::test]
    async fn convert_real_docx_produces_pdf_bytes() {
        const DOCX: &[u8] = include_bytes!("../../docs/public/samples/invoice/Invoice.docx");
        let engine = PdfEngine::new();
        let pdf = engine
            .convert_docx_to_pdf(Bytes::from_static(DOCX))
            .await
            .expect("dxpdf should render the sample invoice");
        assert!(pdf.starts_with(b"%PDF-"), "output should be a PDF");
        assert!(pdf.len() > 1024, "PDF should be non-trivial in size");
    }

    #[tokio::test]
    async fn convert_garbage_returns_error() {
        let engine = PdfEngine::new();
        let result = engine
            .convert_docx_to_pdf(Bytes::from_static(b"not a docx"))
            .await;
        assert!(result.is_err());
    }
}
