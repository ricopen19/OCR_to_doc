use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::GenericImageView;

use crate::ollama::client::OllamaClient;
use super::pdf_to_images::{is_pdf_file, is_image_file, pdf_to_page_images_range};

const OCR_MODEL: &str = "glm-ocr";
const OCR_PROMPT: &str = "OCR";

/// glm-ocr の画像サイズ上限（長辺 2048px）
const MAX_IMAGE_DIMENSION: u32 = 2048;

/// glm-ocr (ViT patch_size=14) が要求する画像サイズのアライメント。
/// 縦横が 28 の倍数でないと GGML_ASSERT エラーが発生する。
const IMAGE_ALIGN: u32 = 28;

/// OCR 処理の進捗コールバック
pub type ProgressCallback = Box<dyn Fn(u32, u32, &str) + Send + Sync>;

/// OCR オプション
pub struct OcrOptions {
    pub ocr_model: String,
    pub dpi: u32,
    pub poppler_path: Option<PathBuf>,
    pub enable_figure: bool,
    /// 図表抽出用 Python バイナリパス
    pub python_bin: Option<String>,
    /// 図表抽出スクリプトのパス (detect_figures.py)
    pub detect_figures_script: Option<PathBuf>,
    /// PDF の開始ページ (1-indexed, None で先頭から)
    pub start_page: Option<u32>,
    /// PDF の終了ページ (1-indexed, None で末尾まで)
    pub end_page: Option<u32>,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            ocr_model: OCR_MODEL.to_string(),
            dpi: 300,
            poppler_path: None,
            enable_figure: true,
            python_bin: None,
            detect_figures_script: None,
            start_page: None,
            end_page: None,
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
        pdf_to_page_images_range(
            input_path,
            result_dir,
            options.dpi,
            options.poppler_path.as_deref(),
            options.start_page,
            options.end_page,
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

        // 画像を読み込み、必要ならリサイズして base64 エンコード
        let image_base64 = encode_image_for_ocr(image_path)?;

        // Ollama で OCR
        let markdown = client
            .chat_vision(&options.ocr_model, OCR_PROMPT, &image_base64)
            .await?;

        // page_###.md に保存
        let md_path = result_dir.join(format!("page_{page_num:03}.md"));
        fs::write(&md_path, &markdown)
            .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;

        // 図表抽出 (YOLOv8x-DocLayNet via Python)
        if options.enable_figure {
            if let (Some(py_bin), Some(script)) =
                (&options.python_bin, &options.detect_figures_script)
            {
                if script.exists() {
                    if let Some(cb) = &on_progress {
                        cb(page_num, total, &format!("図表検出中: {page_num}/{total}"));
                    }
                    match super::figure_extraction::extract_figures(
                        image_path,
                        result_dir,
                        page_num,
                        py_bin,
                        script,
                    ) {
                        Ok(fig_paths) if !fig_paths.is_empty() => {
                            log::info!("Page {page_num}: {} 件の図を抽出", fig_paths.len());
                            append_figure_links(&md_path, &fig_paths);
                        }
                        Err(e) => {
                            log::warn!("Page {page_num}: 図表抽出失敗（続行）: {e}");
                        }
                        _ => {}
                    }
                }
            }
        }

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

/// 画像を読み込み、glm-ocr に適したサイズにリサイズして base64 エンコード。
/// - 長辺を MAX_IMAGE_DIMENSION 以下に収める
/// - 縦横を IMAGE_ALIGN (28) の倍数に揃える（ViT patch_size=14 の制約）
fn encode_image_for_ocr(path: &Path) -> Result<String, String> {
    let img = image::open(path)
        .map_err(|e| format!("画像読み込み失敗 {}: {e}", path.display()))?;
    let (w, h) = img.dimensions();

    // 長辺が上限を超える場合はスケールダウン
    let (mut new_w, mut new_h) = if w > MAX_IMAGE_DIMENSION || h > MAX_IMAGE_DIMENSION {
        let scale = MAX_IMAGE_DIMENSION as f64 / w.max(h) as f64;
        ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
    } else {
        (w, h)
    };

    // 28 の倍数に切り下げ
    new_w = (new_w / IMAGE_ALIGN) * IMAGE_ALIGN;
    new_h = (new_h / IMAGE_ALIGN) * IMAGE_ALIGN;
    // 最低 28px は確保
    new_w = new_w.max(IMAGE_ALIGN);
    new_h = new_h.max(IMAGE_ALIGN);

    if new_w == w && new_h == h {
        // リサイズ不要 — 元ファイルをそのままエンコード
        let bytes = fs::read(path)
            .map_err(|e| format!("画像読み込み失敗: {e}"))?;
        return Ok(BASE64.encode(&bytes));
    }

    log::info!(
        "画像リサイズ: {}x{} → {}x{} ({})",
        w, h, new_w, new_h, path.display()
    );
    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);

    let mut buf = Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("リサイズ画像のエンコード失敗: {e}"))?;
    Ok(BASE64.encode(buf.into_inner()))
}

/// page_*.md の末尾に図表画像へのリンクを追記する。
fn append_figure_links(md_path: &Path, fig_paths: &[PathBuf]) {
    use std::io::Write;
    let Ok(mut file) = fs::OpenOptions::new().append(true).open(md_path) else {
        return;
    };
    let _ = writeln!(file);
    for fig in fig_paths {
        if let Some(name) = fig.file_name().and_then(|n| n.to_str()) {
            let _ = writeln!(file, "![{name}](figures/{name})");
        }
    }
}
