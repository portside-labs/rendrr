//! Upload-time template validation: extension check, size check, then
//! structural + syntactic validation via `docx::template_validator`.

use crate::docx;
use crate::errors::TemplateError;
use crate::models::template::has_docx_extension;
use bytes::Bytes;

const MAX_TEMPLATE_SIZE: u64 = 25 * 1024 * 1024;

pub struct TemplateParser;

impl TemplateParser {
    /// Validate an uploaded template. Rejects anything that isn't a `.docx`
    /// under the size limit whose Handlebars expressions parse cleanly.
    pub fn validate(filename: &str, data: &Bytes) -> Result<(), TemplateError> {
        if !has_docx_extension(filename) {
            return Err(TemplateError::InvalidFileType(
                "File must have a .docx extension".to_string(),
            ));
        }

        let file_size = data.len() as u64;
        if file_size > MAX_TEMPLATE_SIZE {
            return Err(TemplateError::FileTooLarge {
                size: file_size,
                max_size: MAX_TEMPLATE_SIZE,
            });
        }

        docx::template_validator::validate(data)
    }
}
