use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub image_as_pdf: bool,
    #[serde(default)]
    pub enable_figure: bool,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub docx_engine: Option<String>,
    #[serde(default)]
    pub excel_mode: Option<String>,
    #[serde(default)]
    pub use_gpu: bool,
    #[serde(default)]
    pub output_root: Option<String>,
    #[serde(default = "default_excel_meta_sheet")]
    pub excel_meta_sheet: bool,
    #[serde(default = "default_excel_symbol_fallback")]
    pub excel_symbol_fallback: bool,
    #[serde(default)]
    pub chunk_size: Option<u32>,
    #[serde(default)]
    pub enable_rest: bool,
    #[serde(default)]
    pub rest_seconds: Option<u32>,
    #[serde(default)]
    pub pdf_dpi: Option<u32>,
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

fn default_excel_symbol_fallback() -> bool {
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
        image_as_pdf: false,
        enable_figure: true,
        mode: Some("lite".into()),
        docx_engine: Some("python".into()),
        excel_mode: Some("layout".into()),
        use_gpu: false,
        output_root: None,
        excel_meta_sheet: true,
        excel_symbol_fallback: true,
        chunk_size: Some(10),
        enable_rest: false,
        rest_seconds: Some(10),
        pdf_dpi: Some(200),
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
