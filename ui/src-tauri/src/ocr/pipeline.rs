use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::GenericImageView;

use crate::ollama::client::OllamaClient;
use super::pdf_to_images::{
    is_pdf_file, is_image_file,
    pdf_page_count, pdf_single_page_to_image,
};

const OCR_MODEL: &str = "glm-ocr";
const OCR_PROMPT: &str = "OCR";

/// OCR 処理の進捗コールバック
pub type ProgressCallback = Box<dyn Fn(u32, u32, &str) + Send + Sync>;

/// OCR オプション
pub struct OcrOptions {
    pub ocr_model: String,
    pub dpi: u32,
    pub poppler_path: Option<PathBuf>,
    pub enable_figure: bool,
    pub python_bin: Option<String>,
    pub detect_figures_script: Option<PathBuf>,
    pub start_page: Option<u32>,
    pub end_page: Option<u32>,
    pub enable_rest: bool,
    pub rest_seconds: u64,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            ocr_model: OCR_MODEL.to_string(),
            dpi: 300,
            poppler_path: None,
            enable_figure: false,
            python_bin: None,
            detect_figures_script: None,
            start_page: None,
            end_page: None,
            enable_rest: false,
            rest_seconds: 10,
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

    if !client.health_check().await? {
        return Err("Ollama が起動していません。Ollama を起動してください。".to_string());
    }
    if !client.has_model(&options.ocr_model).await? {
        return Err(format!(
            "OCR モデル '{}' が見つかりません。'ollama pull {}' を実行してください。",
            options.ocr_model, options.ocr_model
        ));
    }

    let mut md_paths = Vec::new();

    if is_pdf_file(input_path) {
        // ページ数を取得してページ単位で変換・OCR する
        let total_pages = pdf_page_count(input_path, options.poppler_path.as_deref())?;
        let range_start = options.start_page.unwrap_or(1).max(1).min(total_pages);
        let range_end = options.end_page.unwrap_or(total_pages).max(range_start).min(total_pages);
        let total = range_end - range_start + 1;

        for (i, absolute_page) in (range_start..=range_end).enumerate() {
            let relative_page = (i + 1) as u32;

            if let Some(cb) = &on_progress {
                cb(relative_page, total, &format!("PDF変換中: {relative_page}/{total}ページ"));
            }

            // 1ページだけ変換（CPU バーストを分散）
            let image_path = pdf_single_page_to_image(
                input_path,
                result_dir,
                options.dpi,
                options.poppler_path.as_deref(),
                absolute_page,
                relative_page,
            )?;

            if let Some(cb) = &on_progress {
                cb(relative_page, total, &format!("OCR 処理中: {relative_page}/{total}"));
            }

            let md_path = ocr_image_to_md(
                &image_path, result_dir, relative_page, total,
                options, &client, on_progress,
            ).await?;

            // ページ画像を即削除（メモリ・ディスク節約）
            let _ = fs::remove_file(&image_path);

            md_paths.push(md_path);

            // ページ間休止（最終ページを除く）
            if options.enable_rest && i + 1 < total as usize {
                tokio::time::sleep(tokio::time::Duration::from_secs(options.rest_seconds)).await;
            }
        }

        // page_images フォルダを削除
        let images_dir = result_dir.join("page_images");
        if images_dir.exists() {
            let _ = fs::remove_dir_all(&images_dir);
        }

        if let Some(cb) = on_progress {
            cb(total, total, "OCR 完了");
        }

    } else if is_image_file(input_path) {
        if let Some(cb) = &on_progress {
            cb(1, 1, "OCR 処理中: 1/1");
        }

        let md_path = ocr_image_to_md(
            input_path, result_dir, 1, 1,
            options, &client, on_progress,
        ).await?;
        md_paths.push(md_path);

        if let Some(cb) = on_progress {
            cb(1, 1, "OCR 完了");
        }

    } else {
        return Err(format!("未対応のファイル形式です: {}", input_path.display()));
    }

    Ok(md_paths)
}

/// 1枚の画像を OCR して page_NNN.md に保存する。
async fn ocr_image_to_md(
    image_path: &Path,
    result_dir: &Path,
    page_num: u32,
    total: u32,
    options: &OcrOptions,
    client: &OllamaClient,
    on_progress: Option<&ProgressCallback>,
) -> Result<PathBuf, String> {
    let image_base64 = encode_image_for_ocr(image_path)?;

    let markdown = client
        .chat_vision(&options.ocr_model, OCR_PROMPT, &image_base64)
        .await?;

    let md_path = result_dir.join(format!("page_{page_num:03}.md"));
    fs::write(&md_path, &markdown)
        .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;

    if options.enable_figure {
        if let (Some(py_bin), Some(script)) =
            (&options.python_bin, &options.detect_figures_script)
        {
            if script.exists() {
                if let Some(cb) = &on_progress {
                    cb(page_num, total, &format!("図表検出中: {page_num}/{total}"));
                }
                match super::figure_extraction::extract_figures(
                    image_path, result_dir, page_num, py_bin, script,
                ) {
                    Ok(fig_paths) if !fig_paths.is_empty() => {
                        log::info!("Page {page_num}: {} 件の図を抽出", fig_paths.len());
                        append_figure_links(&md_path, &fig_paths);
                    }
                    Err(e) => log::warn!("Page {page_num}: 図表抽出失敗（続行）: {e}"),
                    _ => {}
                }
            }
        }
    }

    Ok(md_path)
}

/// glm-ocr が安定して処理できる画像サイズの上限（長辺）。
const MAX_IMAGE_DIMENSION: u32 = 1792;

/// 画像を glm-ocr に適したサイズにリサイズして base64 エンコード。
/// リサイズフィルタは Triangle（高速・OCR 用途で Lanczos3 と差なし）。
fn encode_image_for_ocr(path: &Path) -> Result<String, String> {
    let img = image::open(path)
        .map_err(|e| format!("画像読み込み失敗 {}: {e}", path.display()))?;
    let (w, h) = img.dimensions();

    if w <= MAX_IMAGE_DIMENSION && h <= MAX_IMAGE_DIMENSION {
        let bytes = fs::read(path)
            .map_err(|e| format!("画像読み込み失敗: {e}"))?;
        return Ok(BASE64.encode(&bytes));
    }

    let scale = MAX_IMAGE_DIMENSION as f64 / w.max(h) as f64;
    let new_w = (w as f64 * scale) as u32;
    let new_h = (h as f64 * scale) as u32;
    log::info!(
        "画像リサイズ: {}x{} → {}x{} ({})",
        w, h, new_w, new_h, path.display()
    );
    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);

    let mut buf = Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("リサイズ画像のエンコード失敗: {e}"))?;
    drop(resized); // ピクセルバッファを base64 エンコード前に解放
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
