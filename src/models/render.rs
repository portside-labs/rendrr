use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Docx,
    Pdf,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Docx => "docx",
            OutputFormat::Pdf => "pdf",
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            OutputFormat::Docx => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            OutputFormat::Pdf => "application/pdf",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Render {
    pub render_id: Uuid,
    pub template_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub output_format: OutputFormat,
    pub storage_path: String,
    pub file_size_bytes: u64,
    pub render_status: RenderStatus,
    pub error_message: Option<String>,
}

impl Render {
    pub fn new(template_id: Uuid, output_format: OutputFormat) -> Self {
        let render_id = Uuid::now_v7();
        let storage_path = format!("renders/{}.{}", render_id, output_format.extension());

        Self {
            render_id,
            template_id,
            created_at: Utc::now(),
            output_format,
            storage_path,
            file_size_bytes: 0,
            render_status: RenderStatus::Pending,
            error_message: None,
        }
    }
}
