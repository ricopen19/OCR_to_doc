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

const OCR_MODEL: &str = "hf.co/sahilchachra/Unlimited-OCR-GGUF:Q4_K_M";

/// モデルごとの推奨プロンプトを返す。
/// Unlimited OCR は "<image>" トークンを含む専用プロンプトが必要。
fn ocr_prompt_for(model: &str) -> &'static str {
    if model.contains("Unlimited-OCR") {
        "<image>document parsing."
    } else {
        "OCR"
    }
}

/// 出力が Unlimited OCR のトークン形式（座標付き構造体）かを判定する。
/// 先頭の非空行が "title [" / "text [" 等で始まる場合に true を返す。
fn is_unlimited_ocr_format(s: &str) -> bool {
    const TOKENS: &[&str] = &[
        "title [", "text [", "table [", "header [", "section_header [",
        "footer [", "page_number [", "caption [", "figure [",
        "aside_text [", "list_item [", "image [",
    ];
    let first = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    TOKENS.iter().any(|t| first.starts_with(t))
}

/// HTML テーブルの <td>/<th> テキストを抽出して Markdown テーブル形式に変換する。
/// モデルが <tr> を省略した不正 HTML を出力する場合でも全セルを 1 行として扱う。
fn html_table_to_markdown(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut cells: Vec<String> = Vec::new();
    let mut pos = 0;

    loop {
        let td = lower[pos..].find("<td").map(|i| i + pos);
        let th = lower[pos..].find("<th").map(|i| i + pos);
        let tag_start = match (td, th) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => break,
        };
        let tag_start = tag_start.unwrap();

        // 開始タグの > を探す
        let after_open = match html[tag_start..].find('>') {
            Some(i) => tag_start + i + 1,
            None => break,
        };

        // 次のセルまたは </table> を探してセル内容の終端を決める
        let rest_lower = &lower[after_open..];
        let end = [
            rest_lower.find("<td"),
            rest_lower.find("<th"),
            rest_lower.find("</td"),
            rest_lower.find("</th"),
            rest_lower.find("</table"),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest_lower.len());

        let cell_html = &html[after_open..after_open + end];
        let cell_text = strip_html_tags(cell_html);
        let cell_text = cell_text.trim().replace('|', "｜");
        if !cell_text.is_empty() {
            cells.push(cell_text);
        }

        pos = after_open + end;
    }

    if cells.is_empty() {
        return String::new();
    }

    let row = format!("| {} |", cells.join(" | "));
    let divider = format!("| {} |", vec!["---"; cells.len()].join(" | "));
    format!("{row}\n{divider}")
}

/// HTML タグを除去してテキストだけを返す。
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Unlimited OCR のトークン形式を Markdown に変換する。
/// - title / header / section_header → 見出し
/// - text / caption / list_item / aside_text → 本文
/// - table → <td> を抽出して Markdown テーブル形式に変換
/// - footer / page_number / figure / image → スキップ
fn unlimited_ocr_to_markdown(raw: &str) -> String {
    const ELEM_TYPES: &[&str] = &[
        "title", "header", "section_header", "text", "caption",
        "table", "footer", "page_number", "figure", "aside_text", "list_item", "image",
    ];

    let mut result = String::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let matched = ELEM_TYPES.iter().find_map(|&t| {
            line.strip_prefix(&format!("{t} ")).map(|rest| (t, rest))
        });

        let (elem_type, rest) = match matched {
            Some(m) => m,
            None => {
                result.push_str(line);
                result.push_str("\n\n");
                continue;
            }
        };

        // "[x1, y1, x2, y2]content" → content
        let content = if rest.starts_with('[') {
            rest.find(']').map(|i| rest[i + 1..].trim()).unwrap_or(rest)
        } else {
            rest.trim()
        };

        if content.is_empty() {
            continue;
        }

        match elem_type {
            "title" => {
                result.push_str("# ");
                result.push_str(content);
                result.push_str("\n\n");
            }
            "header" | "section_header" => {
                result.push_str("## ");
                result.push_str(content);
                result.push_str("\n\n");
            }
            "text" | "caption" | "list_item" => {
                result.push_str(content);
                result.push_str("\n\n");
            }
            "aside_text" => {
                result.push_str("> ");
                result.push_str(content);
                result.push_str("\n\n");
            }
            "table" => {
                let md_table = html_table_to_markdown(content);
                if !md_table.is_empty() {
                    result.push_str(&md_table);
                    result.push_str("\n\n");
                }
            }
            // footer / page_number / figure / image はスキップ
            _ => {}
        }
    }

    result
}

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

    let prompt = ocr_prompt_for(&options.ocr_model);
    let raw_ocr = client
        .chat_vision(&options.ocr_model, prompt, &image_base64)
        .await?;
    let markdown = if is_unlimited_ocr_format(&raw_ocr) {
        unlimited_ocr_to_markdown(&raw_ocr)
    } else {
        raw_ocr
    };

    let md_path = result_dir.join(format!("page_{page_num:03}.md"));
    fs::write(&md_path, &markdown)
        .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;

    if options.enable_figure {
        if let (Some(py_bin), Some(script)) =
            (&options.python_bin, &options.detect_figures_script)
        {
            if script.exists() {
                if let Some(cb) = &on_progress {
                    cb(page_num, total, &format!("図表検出中: {page_num}/{total}（初回はモデルDL数分かかる場合あり）"));
                }
                let py_bin_c = py_bin.clone();
                let script_c = script.clone();
                let img_c = image_path.to_path_buf();
                let dir_c = result_dir.to_path_buf();
                let result = tokio::time::timeout(
                    tokio::time::Duration::from_secs(600),
                    tokio::task::spawn_blocking(move || {
                        super::figure_extraction::extract_figures(
                            &img_c, &dir_c, page_num, &py_bin_c, &script_c,
                        )
                    }),
                ).await;
                match result {
                    Ok(Ok(Ok(fig_paths))) if !fig_paths.is_empty() => {
                        log::info!("Page {page_num}: {} 件の図を抽出", fig_paths.len());
                        append_figure_links(&md_path, &fig_paths);
                    }
                    Ok(Ok(Err(e))) => log::warn!("Page {page_num}: 図表抽出失敗（続行）: {e}"),
                    Err(_) => log::warn!("Page {page_num}: 図表抽出タイムアウト（スキップ）"),
                    _ => {}
                }
            }
        }
    }

    Ok(md_path)
}

/// OCR モデルに送る画像の長辺上限。これ以上は縮小してから送る。
const MAX_IMAGE_DIMENSION: u32 = 1792;

/// 画像を OCR モデル向けにリサイズして base64 エンコード。
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
