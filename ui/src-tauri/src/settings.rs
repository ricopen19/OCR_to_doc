use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub enable_figure: bool,
    #[serde(default)]
    pub docx_engine: Option<String>,
    #[serde(default)]
    pub excel_mode: Option<String>,
    #[serde(default)]
    pub output_root: Option<String>,
    #[serde(default = "default_excel_meta_sheet")]
    pub excel_meta_sheet: bool,
    #[serde(default)]
    pub enable_rest: bool,
    #[serde(default)]
    pub rest_seconds: Option<u32>,
    #[serde(default)]
    pub pdf_dpi: Option<u32>,
    /// OCR エンジン識別子（"ollama" | "llamacpp"）。
    #[serde(default)]
    pub ocr_engine: Option<String>,
    /// 使用する OCR モデル名（未指定は既定モデル）。
    #[serde(default)]
    pub ocr_model: Option<String>,
    /// llama.cpp のベース URL（エンジンが llamacpp のときのみ）。
    #[serde(default)]
    pub llama_base_url: Option<String>,
    /// llama.cpp が認証を要求する場合の API キー。
    #[serde(default)]
    pub llama_api_key: Option<String>,
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    #[serde(default = "default_preview_quality")]
    pub preview_quality: String,
}

fn default_excel_meta_sheet() -> bool {
    true
}

fn default_preview_quality() -> String {
    "light".to_string()
}

pub fn load_settings_from_disk(
    project_root: &Path,
    config_dir: &Path,
) -> Result<AppSettings, String> {
    if !config_dir.exists() {
        let _ = fs::create_dir_all(config_dir);
    }

    let settings_path = config_dir.join("settings.json");
    if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        let settings: AppSettings = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        return Ok(settings);
    }

    // レガシーパスのフォールバック
    let legacy_path = project_root.join("configs").join("settings.json");
    if legacy_path.exists() {
        let content = fs::read_to_string(&legacy_path).map_err(|e| e.to_string())?;
        let settings: AppSettings = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        return Ok(settings);
    }

    // デフォルト値
    Ok(AppSettings {
        formats: vec!["md".into()],
        enable_figure: false,
        docx_engine: Some("python".into()),
        excel_mode: Some("layout".into()),
        output_root: None,
        excel_meta_sheet: true,
        enable_rest: true,
        rest_seconds: Some(5),
        pdf_dpi: Some(150),
        ocr_engine: Some("ollama".into()),
        ocr_model: Some("glm-ocr".into()),
        llama_base_url: None,
        llama_api_key: None,
        window_width: Some(1200),
        window_height: Some(760),
        preview_quality: default_preview_quality(),
    })
}

pub fn save_settings_to_disk(settings: &AppSettings, config_dir: &Path) -> Result<(), String> {
    if !config_dir.exists() {
        fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    }
    let settings_path = config_dir.join("settings.json");
    let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path, content).map_err(|e| e.to_string())?;
    Ok(())
}
