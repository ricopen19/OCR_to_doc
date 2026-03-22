use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::ollama::client::OllamaClient;
use super::pdf_to_images::{is_pdf_file, is_image_file};

const OCR_MODEL: &str = "glm-ocr";
const OCR_PROMPT: &str = "この画像のテキストを Markdown 形式で出力してください。\
テーブルは Markdown テーブル構文を使い、数式はプレーンテキストで表現してください。\
画像内の図やイラストは無視し、テキスト情報のみ抽出してください。";

/// OCR 処理の進捗コールバック
pub type ProgressCallback = Box<dyn Fn(u32, u32, &str) + Send + Sync>;

/// OCR オプション
pub struct OcrOptions {
    pub ocr_model: String,
    pub dpi: u32,
    pub poppler_path: Option<PathBuf>,
    pub enable_figure: bool,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            ocr_model: OCR_MODEL.to_string(),
            dpi: 300,
            poppler_path: None,
            enable_figure: true,
        }
    }
}

/// 入力ファイル（PDF or 画像）を OCR し、result_dir に page_###.md を生成する。
pub async fn run_ocr_pipeline(
    input_path: &Path,
    result_dir: &Path,
    options: &OcrOptions,
    on_progress: Option<&ProgressCallback>,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(result_dir)
        .map_err(|e| format!("出力ディレクトリ作成失敗: {e}"))?;

    let client = OllamaClient::new();

    // Ollama 起動確認
    if !client.health_check().await? {
        return Err("Ollama が起動していません。Ollama を起動してください。".to_string());
    }

    // モデル確認
    if !client.has_model(&options.ocr_model).await? {
        return Err(format!(
            "OCR モデル '{}' が見つかりません。'ollama pull {}' を実行してください。",
            options.ocr_model, options.ocr_model
        ));
    }

    // 入力種別で分岐
    let page_images = if is_pdf_file(input_path) {
        super::pdf_to_images::pdf_to_page_images(
            input_path,
            result_dir,
            options.dpi,
            options.poppler_path.as_deref(),
        )?
    } else if is_image_file(input_path) {
        vec![input_path.to_path_buf()]
    } else {
        return Err(format!(
            "未対応のファイル形式です: {}",
            input_path.display()
        ));
    };

    let total = page_images.len() as u32;
    let mut md_paths = Vec::new();

    for (i, image_path) in page_images.iter().enumerate() {
        let page_num = (i + 1) as u32;

        if let Some(cb) = &on_progress {
            cb(page_num, total, &format!("OCR 処理中: {page_num}/{total}"));
        }

        // 画像を base64 エンコード
        let image_bytes = fs::read(image_path)
            .map_err(|e| format!("画像読み込み失敗 {}: {e}", image_path.display()))?;
        let image_base64 = BASE64.encode(&image_bytes);

        // Ollama で OCR
        let markdown = client
            .chat_vision(&options.ocr_model, OCR_PROMPT, &image_base64)
            .await?;

        // page_###.md に保存
        let md_path = result_dir.join(format!("page_{page_num:03}.md"));
        fs::write(&md_path, &markdown)
            .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;
        md_paths.push(md_path);
    }

    // PDF の場合、page_images を削除
    if is_pdf_file(input_path) {
        let images_dir = result_dir.join("page_images");
        if images_dir.exists() {
            let _ = fs::remove_dir_all(&images_dir);
        }
    }

    if let Some(cb) = &on_progress {
        cb(total, total, "OCR 完了");
    }

    Ok(md_paths)
}

/// 画像ファイルを base64 エンコードする
pub fn encode_image_base64(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("画像読み込み失敗: {e}"))?;
    Ok(BASE64.encode(&bytes))
}
