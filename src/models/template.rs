use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The only template format Rendrr accepts. Kept as a constant rather than a
/// `TemplateKind` enum because the service is DOCX-only; see the format matrix
/// in CLAUDE.md.
pub const TEMPLATE_EXTENSION: &str = "docx";

pub const TEMPLATE_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// True when `filename` has a `.docx` extension (case-insensitive).
pub fn has_docx_extension(filename: &str) -> bool {
    filename.to_lowercase().ends_with(".docx")
}

/// Storage key for a template object.
pub fn template_storage_path(template_id: &Uuid) -> String {
    format!("templates/{}.{}", template_id, TEMPLATE_EXTENSION)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub template_id: Uuid,
    pub name: String,
    pub original_filename: String,
    pub storage_path: String,
    pub uploaded_at: DateTime<Utc>,
    pub file_size_bytes: u64,
    pub content_type: String,
}

impl Template {
    pub fn new(name: Option<String>, original_filename: String, file_size_bytes: u64) -> Self {
        let template_id = Uuid::now_v7();
        let storage_path = template_storage_path(&template_id);
        let name = name.unwrap_or_else(|| original_filename.clone());

        Self {
            template_id,
            name,
            original_filename,
            storage_path,
            uploaded_at: Utc::now(),
            file_size_bytes,
            content_type: TEMPLATE_CONTENT_TYPE.to_string(),
        }
    }
}
