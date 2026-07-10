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
/// 表の高精度再OCR に使うモデル（Unlimited OCR は表を <tr>/<td> なしで出力するため）。
const TABLE_OCR_MODEL: &str = "glm-ocr";

/// Unlimited OCR が出力した表領域。bbox はページ画像に対して 0-1000 正規化された座標。
struct TableRegion {
    bbox: (u32, u32, u32, u32),
    html: String,
}

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
        // <td>/<th> が1つも取れない（Unlimited OCR の不正 HTML 等）場合は、
        // 表構造を諦めて平坦テキストとして内容だけ保持する。
        return strip_html_tags(html).trim().to_string();
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

/// "[x1, y1, x2, y2]..." 形式の先頭から bbox の4値を読み取る。
/// 形式が崩れている（'[' がない・値が4つでない等）場合は None。
fn parse_table_bbox(rest: &str) -> Option<(u32, u32, u32, u32)> {
    if !rest.starts_with('[') {
        return None;
    }
    let end = rest.find(']')?;
    let inner = &rest[1..end];
    let parts: Vec<f64> = inner
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    if parts.len() != 4 {
        return None;
    }
    Some((
        parts[0].round() as u32,
        parts[1].round() as u32,
        parts[2].round() as u32,
        parts[3].round() as u32,
    ))
}

/// Unlimited OCR のトークン形式を Markdown に変換する。
/// - title / header / section_header → 見出し
/// - text / caption / list_item / aside_text → 本文
/// - table → bbox が取れればプレースホルダを挿入し TableRegion として収集
///   （呼び出し側が再OCR または平坦テキストで置換する）。bbox が取れなければ
///   その場で平坦テキスト変換する。
/// - footer / page_number / figure / image → スキップ
fn unlimited_ocr_to_markdown(raw: &str) -> (String, Vec<TableRegion>) {
    const ELEM_TYPES: &[&str] = &[
        "title", "header", "section_header", "text", "caption",
        "table", "footer", "page_number", "figure", "aside_text", "list_item", "image",
    ];

    let mut result = String::new();
    let mut table_regions: Vec<TableRegion> = Vec::new();

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
            "table" => match parse_table_bbox(rest) {
                Some(bbox) => {
                    let idx = table_regions.len();
                    table_regions.push(TableRegion { bbox, html: content.to_string() });
                    result.push_str(&format!("<!--TABLE_REOCR_{idx}-->"));
                    result.push_str("\n\n");
                }
                None => {
                    let md_table = html_table_to_markdown(content);
                    if !md_table.is_empty() {
                        result.push_str(&md_table);
                        result.push_str("\n\n");
                    }
                }
            },
            // footer / page_number / figure / image はスキップ
            _ => {}
        }
    }

    (result, table_regions)
}

/// glm-ocr の出力から Markdown テーブル部分だけを取り出す。
/// glm-ocr は同じ表を「プレーン出力 → ```table フェンス付き再出力」の順で
/// 二重に返す癖があるため、フェンス内を優先的に採用して重複を除去する。
fn extract_table_markdown(raw: &str) -> String {
    if let Some(start) = raw.find("```table") {
        let after = &raw[start + "```table".len()..];
        let fenced = match after.find("```") {
            Some(end) => &after[..end],
            None => after,
        };
        return fenced.trim().to_string();
    }

    // フェンスがなければ最初の Markdown テーブルブロック（| 始まりの連続行）を抽出
    let mut block: Vec<&str> = Vec::new();
    let mut started = false;
    for line in raw.lines() {
        if line.trim().starts_with('|') {
            block.push(line);
            started = true;
        } else if started {
            break;
        }
    }
    if !block.is_empty() {
        return block.join("\n").trim().to_string();
    }

    raw.trim().to_string()
}

/// 0-1000 正規化された bbox をページ画像から切り出す。各辺に 1%（座標値で ±10）の
/// パディングを加え、罫線が欠けないようにする（画像端でクランプ）。
fn crop_table_region(img: &image::DynamicImage, bbox: (u32, u32, u32, u32)) -> image::DynamicImage {
    const PAD: u32 = 10;
    let (width, height) = img.dimensions();
    let (x1, y1, x2, y2) = bbox;

    let px1 = x1.saturating_sub(PAD).min(1000);
    let py1 = y1.saturating_sub(PAD).min(1000);
    let px2 = (x2 + PAD).min(1000);
    let py2 = (y2 + PAD).min(1000);

    let to_px_x = |v: u32| (v as u64 * width as u64 / 1000) as u32;
    let to_px_y = |v: u32| (v as u64 * height as u64 / 1000) as u32;

    let crop_x1 = to_px_x(px1).min(width.saturating_sub(1));
    let crop_y1 = to_px_y(py1).min(height.saturating_sub(1));
    let crop_x2 = to_px_x(px2).max(crop_x1 + 1).min(width);
    let crop_y2 = to_px_y(py2).max(crop_y1 + 1).min(height);

    img.crop_imm(crop_x1, crop_y1, crop_x2 - crop_x1, crop_y2 - crop_y1)
}

/// glm-ocr (ViT patch_size=14) は画像の縦横が 28 の倍数でないと GGML_ASSERT で落ちるため、
/// 表クロップ画像はこの倍数に切り下げてから送る。
const TABLE_IMAGE_ALIGN: u32 = 28;

/// 切り出した表画像を glm-ocr 向けにリサイズ・アライメントして base64 エンコードする。
fn encode_table_crop_for_ocr(img: image::DynamicImage) -> Result<String, String> {
    let (w, h) = img.dimensions();
    let (mut new_w, mut new_h) = if w > MAX_IMAGE_DIMENSION || h > MAX_IMAGE_DIMENSION {
        let scale = MAX_IMAGE_DIMENSION as f64 / w.max(h) as f64;
        ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
    } else {
        (w, h)
    };
    new_w = ((new_w / TABLE_IMAGE_ALIGN).max(1)) * TABLE_IMAGE_ALIGN;
    new_h = ((new_h / TABLE_IMAGE_ALIGN).max(1)) * TABLE_IMAGE_ALIGN;

    let resized = if new_w == w && new_h == h {
        img
    } else {
        img.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle)
    };

    let mut buf = Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("表画像のエンコード失敗: {e}"))?;
    Ok(BASE64.encode(buf.into_inner()))
}

/// 1つの表領域を切り出して glm-ocr で再OCR し、Markdown テーブルを返す。
async fn reocr_single_table(
    base_img: &image::DynamicImage,
    region: &TableRegion,
    client: &OllamaClient,
) -> Result<String, String> {
    let cropped = crop_table_region(base_img, region.bbox);
    let image_base64 = encode_table_crop_for_ocr(cropped)?;

    // glm-ocr のコールドロード（Unlimited OCR 常駐下でのモデル入れ替え）は
    // 60秒を超えることがあるため、図表抽出と同等の余裕を持たせる
    let raw = tokio::time::timeout(
        tokio::time::Duration::from_secs(300),
        client.chat_vision(TABLE_OCR_MODEL, "OCR", &image_base64),
    )
    .await
    .map_err(|_| "glm-ocr 再OCR タイムアウト（300秒）".to_string())??;

    let table_md = extract_table_markdown(&raw);
    if table_md.is_empty() {
        return Err("glm-ocr 再OCR 結果が空でした".to_string());
    }
    Ok(table_md)
}

/// markdown 内の `<!--TABLE_REOCR_N-->` プレースホルダをすべて置換する。
/// enable_table_reocr が ON かつ glm-ocr が利用可能なら再OCR結果、
/// それ以外・失敗時は平坦テキストで埋める（プレースホルダを残さない）。
async fn resolve_table_placeholders(
    markdown: String,
    table_regions: Vec<TableRegion>,
    image_path: &Path,
    enable_table_reocr: bool,
    table_reocr_available: bool,
    client: &OllamaClient,
    page_num: u32,
    total: u32,
    on_progress: Option<&ProgressCallback>,
) -> String {
    if table_regions.is_empty() {
        return markdown;
    }

    let use_reocr = enable_table_reocr && table_reocr_available;
    let base_img = if use_reocr {
        image::open(image_path).ok()
    } else {
        None
    };

    let mut result = markdown;
    let mut warned = false;
    for (idx, region) in table_regions.iter().enumerate() {
        let placeholder = format!("<!--TABLE_REOCR_{idx}-->");
        let replacement = match &base_img {
            Some(img) => match reocr_single_table(img, region, client).await {
                Ok(md) => md,
                Err(e) => {
                    log::warn!("Page {page_num}: 表{idx}の再OCR失敗（平坦テキストにフォールバック）: {e}");
                    if !warned {
                        if let Some(cb) = &on_progress {
                            cb(page_num, total, "表の再OCRに失敗したため平坦テキストで出力します");
                        }
                        warned = true;
                    }
                    html_table_to_markdown(&region.html)
                }
            },
            None => html_table_to_markdown(&region.html),
        };
        result = result.replace(&placeholder, &replacement);
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
    pub enable_table_reocr: bool,
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
            enable_table_reocr: false,
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

    // 表の高精度再OCR が ON の場合のみ、glm-ocr の存在を1回だけ確認する。
    // 不在でもエラーにはせず、以降のページで平坦テキストにフォールバックする。
    let table_reocr_available = if options.enable_table_reocr {
        let available = client.has_model(TABLE_OCR_MODEL).await.unwrap_or(false);
        if !available {
            if let Some(cb) = &on_progress {
                cb(0, 0, "glm-ocr が見つからないため表は平坦テキストで出力します");
            }
        }
        available
    } else {
        false
    };

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
                options, &client, table_reocr_available, on_progress,
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
            options, &client, table_reocr_available, on_progress,
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
    table_reocr_available: bool,
    on_progress: Option<&ProgressCallback>,
) -> Result<PathBuf, String> {
    let image_base64 = encode_image_for_ocr(image_path)?;

    let prompt = ocr_prompt_for(&options.ocr_model);
    let raw_ocr = client
        .chat_vision(&options.ocr_model, prompt, &image_base64)
        .await?;
    let markdown = if is_unlimited_ocr_format(&raw_ocr) {
        let (md, table_regions) = unlimited_ocr_to_markdown(&raw_ocr);
        resolve_table_placeholders(
            md, table_regions, image_path,
            options.enable_table_reocr, table_reocr_available,
            client, page_num, total, on_progress,
        ).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_table_to_markdown_falls_back_to_flat_text_without_td() {
        let html = "<table>セル1セル2セル3</table>";
        let result = html_table_to_markdown(html);
        assert_eq!(result, "セル1セル2セル3");
    }

    #[test]
    fn html_table_to_markdown_still_parses_td_when_present() {
        let html = "<table><td>A</td><td>B</td></table>";
        let result = html_table_to_markdown(html);
        assert_eq!(result, "| A | B |\n| --- | --- |");
    }

    #[test]
    fn unlimited_ocr_to_markdown_collects_table_region_with_bbox() {
        let raw = "table [10, 20, 900, 300]<table><td>A</td><td>B</td></table>";
        let (md, regions) = unlimited_ocr_to_markdown(raw);
        assert!(md.contains("<!--TABLE_REOCR_0-->"));
        assert!(!md.contains("TABLE_REOCR_1"));
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].bbox, (10, 20, 900, 300));
        assert_eq!(regions[0].html, "<table><td>A</td><td>B</td></table>");
    }

    #[test]
    fn unlimited_ocr_to_markdown_falls_back_when_bbox_missing() {
        let raw = "table <table><td>A</td></table>";
        let (md, regions) = unlimited_ocr_to_markdown(raw);
        assert!(regions.is_empty());
        assert!(!md.contains("TABLE_REOCR"));
        assert!(md.contains("| A |"));
    }

    #[test]
    fn extract_table_markdown_prefers_fenced_block_and_dedupes() {
        let raw = "| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```table\n| A | B |\n| --- | --- |\n| 1 | 2 |\n```";
        let result = extract_table_markdown(raw);
        assert_eq!(result, "| A | B |\n| --- | --- |\n| 1 | 2 |");
    }

    #[test]
    fn extract_table_markdown_without_fence_extracts_first_block() {
        let raw = "前置きテキスト\n| A | B |\n| --- | --- |\n| 1 | 2 |\n後書きテキスト";
        let result = extract_table_markdown(raw);
        assert_eq!(result, "| A | B |\n| --- | --- |\n| 1 | 2 |");
    }

    #[test]
    fn extract_table_markdown_falls_back_to_raw_when_no_table_found() {
        let raw = "テーブルが見つかりません";
        let result = extract_table_markdown(raw);
        assert_eq!(result, "テーブルが見つかりません");
    }

    /// 実機統合テスト。Ollama 起動中 + Unlimited OCR + glm-ocr が必要。
    /// TABLE_REOCR_SAMPLE に表を含む画像パスを指定して実行する:
    /// TABLE_REOCR_SAMPLE=/path/to/table.png cargo test table_reocr_end_to_end -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn table_reocr_end_to_end() {
        let sample = std::env::var("TABLE_REOCR_SAMPLE")
            .expect("TABLE_REOCR_SAMPLE に表を含む画像パスを指定してください");
        let sample = std::path::PathBuf::from(sample);
        assert!(sample.exists(), "サンプル画像が存在しません: {sample:?}");

        let base = std::env::temp_dir().join(format!("table_reocr_it_{}", std::process::id()));

        for (enable, label) in [(false, "off"), (true, "on")] {
            let result_dir = base.join(label);
            std::fs::create_dir_all(&result_dir).unwrap();
            let options = OcrOptions {
                enable_table_reocr: enable,
                ..Default::default()
            };
            let outputs = run_ocr_pipeline(&sample, &result_dir, &options, None)
                .await
                .unwrap_or_else(|e| panic!("pipeline 失敗 ({label}): {e}"));
            assert!(!outputs.is_empty(), "出力ファイルなし ({label})");
            let md = std::fs::read_to_string(&outputs[0]).unwrap();
            println!("=== enable_table_reocr={label} ===\n{md}\n");
            assert!(
                !md.contains("<!--TABLE_REOCR_"),
                "プレースホルダが残留 ({label})"
            );
            assert!(md.contains("A-001"), "表の内容が消えている ({label})");
            if enable {
                assert!(
                    md.lines().filter(|l| l.trim_start().starts_with('|')).count() >= 3,
                    "ON なのに Markdown テーブルが出力されていない"
                );
            }
        }
    }
}
