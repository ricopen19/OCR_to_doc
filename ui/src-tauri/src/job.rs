use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct AppState {
    pub jobs: Mutex<HashMap<String, JobInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobInfo {
    pub status: JobStatus,
    pub progress: f32,
    pub log: Vec<String>,
    pub outputs: Vec<String>,
    pub output_paths: Vec<String>,
    pub preview: Option<String>,
    pub error: Option<String>,
    pub current_message: Option<String>,
    pub page_current: Option<u32>,
    pub page_total: Option<u32>,
    pub eta_seconds: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Idle,
    Running,
    Done,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub enable_figure: bool,
    #[serde(default)]
    pub docx_engine: Option<String>,
    #[serde(default)]
    pub enable_rest: bool,
    #[serde(default)]
    pub rest_seconds: Option<u32>,
    #[serde(default)]
    pub pdf_dpi: Option<u32>,
    #[serde(default)]
    pub excel_mode: Option<String>,
    #[serde(default)]
    pub excel_meta_sheet: Option<bool>,
    #[serde(default)]
    pub file_options: Option<HashMap<String, FileSpecificOptions>>,
    #[serde(default)]
    pub use_embedded_text: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileSpecificOptions {
    pub start: Option<u32>,
    pub end: Option<u32>,
    pub crop: Option<CropRect>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunJobResponse {
    pub job_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressResponse {
    pub status: JobStatus,
    pub progress: f32,
    pub log: Vec<String>,
    pub error: Option<String>,
    pub current_message: Option<String>,
    pub page_current: Option<u32>,
    pub page_total: Option<u32>,
    pub eta_seconds: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultResponse {
    pub outputs: Vec<String>,
    pub preview: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentResultEntry {
    pub dir_name: String,
    pub updated_at_ms: u64,
    pub page_range: Option<String>,
    pub best_file: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStatus {
    pub project_root: String,
    pub os: String,
    pub dispatcher_found: bool,
    pub dispatcher_path: Option<String>,
    pub result_dir_found: bool,
    pub result_root: String,
    pub python_bin: String,
    pub python_found: bool,
    pub python_path: Option<String>,
    pub poppler_found: bool,
    pub poppler_path: Option<String>,
    pub resource_roots: Vec<String>,
    pub ollama_running: bool,
    pub ocr_model_ready: bool,
    pub ocr_model_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
    pub data_url: String,
    pub page_count: Option<u32>,
    pub page: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfTextDetectionResponse {
    /// "TextBased" / "Scanned" / "ImageBased" / "Mixed"
    pub pdf_type: String,
    pub confidence: f32,
    /// TextBased / Mixed の場合のみ true。埋め込みテキスト使用オプションを提示してよいか。
    pub eligible: bool,
}
