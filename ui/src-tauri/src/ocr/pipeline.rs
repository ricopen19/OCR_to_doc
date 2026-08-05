use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::GenericImageView;
use regex::Regex;

use crate::job::CropRect;
use crate::ollama::client::OllamaClient;
use super::pdf_to_images::{
    is_pdf_file, is_image_file,
    pdf_page_count, pdf_single_page_to_image,
};

pub(crate) const OCR_MODEL: &str = "hf.co/sahilchachra/Unlimited-OCR-GGUF:Q4_K_M";
/// 表の高精度再OCR に使うモデル（Unlimited OCR は表を <tr>/<td> なしで出力するため）。
pub(crate) const TABLE_OCR_MODEL: &str = "glm-ocr";

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

/// HTML テーブルの <tr>/<td>/<th> を解釈して Markdown テーブル形式（複数行）に変換する。
/// <tr> を1つも含まない不正 HTML（Unlimited OCR が稀に出力する）の場合は、
/// 全体を単一行として扱う。
fn html_table_to_markdown(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let row_spans = split_table_rows(html, &lower);

    let rows: Vec<Vec<String>> = row_spans
        .into_iter()
        .map(|(start, end)| extract_row_cells(&html[start..end], &lower[start..end]))
        .filter(|cells| !cells.is_empty())
        .collect();

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if rows.is_empty() || col_count == 0 {
        // <tr>/<td>/<th> が1つも取れない（Unlimited OCR の不正 HTML 等）場合は、
        // 表構造を諦めて平坦テキストとして内容だけ保持する。
        return strip_html_tags(html).trim().to_string();
    }

    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        let mut padded = row.clone();
        padded.resize(col_count, String::new());
        out.push_str(&format!("| {} |\n", padded.join(" | ")));
        if i == 0 {
            out.push_str(&format!("| {} |\n", vec!["---"; col_count].join(" | ")));
        }
    }
    out.trim_end().to_string()
}

/// <tr>...</tr> の範囲（バイトオフセット）を列挙する。<tr> を1つも含まない場合は
/// html 全体を単一行として返す（<tr> を省略する不正 HTML への既存フォールバック）。
fn split_table_rows(html: &str, lower: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut pos = 0;
    while let Some(tr_start) = lower[pos..].find("<tr").map(|i| i + pos) {
        let after_open = match html[tr_start..].find('>') {
            Some(i) => tr_start + i + 1,
            None => break,
        };
        let end = lower[after_open..]
            .find("</tr")
            .map(|i| after_open + i)
            .unwrap_or(html.len());
        spans.push((after_open, end));
        pos = if end > after_open { end } else { after_open + 1 };
    }

    if spans.is_empty() {
        spans.push((0, html.len()));
    }
    spans
}

/// 1行分の HTML 断片から <td>/<th> のテキストを順序通り抽出する。
/// 空セルも列位置を保つために残す（詰めると後続セルが左にずれて列がずれるため）。
fn extract_row_cells(html: &str, lower: &str) -> Vec<String> {
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

        // 次のセルまたは行末を探してセル内容の終端を決める
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
        cells.push(cell_text);

        pos = after_open + end;
    }

    cells
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

/// `\( ... \)` / `\[ ... \]` を Obsidian 等のデフォルト Markdown レンダラが認識する
/// `$ ... $` / `$$ ... $$` に変換する。Unlimited OCR は LaTeX 区切りとして
/// `\(`/`\)`/`\[`/`\]` を出力するが、これらは MathJax の追加設定なしには
/// 数式として描画されないビューアが多いため、書き込み前に統一する。
fn sanitize_math_delimiters(text: &str) -> String {
    let display = Regex::new(r"(?s)\\\[(.*?)\\\]").unwrap();
    let text = display.replace_all(text, |caps: &regex::Captures| format!("$${}$$", &caps[1]));
    let inline = Regex::new(r"(?s)\\\((.*?)\\\)").unwrap();
    let text = inline.replace_all(&text, |caps: &regex::Captures| format!("${}$", &caps[1]));
    text.into_owned()
}

/// Unlimited OCR がまれに OCR 結果ではなく英語の思考・雑談（chain-of-thought の漏れ）を
/// 出力し始めることがある。正規の OCR 出力には現れないはずの定型文言なので、
/// 出現した時点でそこから後ろを切り捨てる。
const THOUGHT_LEAK_MARKERS: &[&str] = &[
    "Wait, looking at",
    "Okay, I'm ready",
    "Final transcription:",
    "Let's re-read",
];

fn truncate_thought_leak(raw: &str) -> String {
    let cut_at = THOUGHT_LEAK_MARKERS
        .iter()
        .filter_map(|marker| raw.find(marker))
        .min();
    match cut_at {
        Some(idx) => raw[..idx].trim_end().to_string(),
        None => raw.to_string(),
    }
}

/// 直前の非空行とどれだけ類似しているかを、文字2-gram の Jaccard 係数で判定する。
/// 完全一致だけでなく、途中の数文字が変化しながら劣化していく暴走生成（repeat_penalty
/// を強めた結果よくある症状）も検知できるよう、部分一致に寛容な指標を使う。
fn lines_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let bigrams = |s: &str| -> std::collections::HashSet<(char, char)> {
        let chars: Vec<char> = s.chars().collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (sa, sb) = (bigrams(a), bigrams(b));
    if sa.is_empty() || sb.is_empty() {
        return false;
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count().max(1);
    (inter as f64 / union as f64) >= 0.5
}

/// repeat_penalty 等のサンプラー対策だけでは抑えきれない暴走生成
/// （同一・酷似した短い行が延々と繰り返される）を検知し、繰り返しが
/// 始まった位置で応答を打ち切る安全網。
fn truncate_runaway_repetition(raw: &str) -> String {
    const REPEAT_THRESHOLD: usize = 3;

    let lines: Vec<&str> = raw.lines().collect();
    let mut prev_idx: Option<usize> = None;
    let mut prev_line: Option<&str> = None;
    let mut run_len = 0usize;
    let mut run_start_idx = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_similar = prev_line.map(|p| lines_similar(p, trimmed)).unwrap_or(false);
        if is_similar {
            if run_len == 0 {
                run_start_idx = prev_idx.unwrap();
            }
            run_len += 1;
            if run_len >= REPEAT_THRESHOLD {
                return lines[..run_start_idx].join("\n").trim_end().to_string();
            }
        } else {
            run_len = 0;
        }
        prev_idx = Some(i);
        prev_line = Some(trimmed);
    }

    raw.to_string()
}

/// Unlimited OCR のトークン形式を Markdown に変換する。
/// - title / header / section_header → 見出し
/// - text / caption / list_item / list / page_footnote / aside_text → 本文
/// - table → bbox が取れればプレースホルダを挿入し TableRegion として収集
///   （呼び出し側が再OCR または平坦テキストで置換する）。bbox が取れなければ
///   その場で平坦テキスト変換する。
/// - footer / page_number / figure / image → スキップ
fn unlimited_ocr_to_markdown(raw: &str) -> (String, Vec<TableRegion>) {
    const ELEM_TYPES: &[&str] = &[
        "title", "header", "section_header", "text", "caption", "image_caption",
        "table", "equation", "footer", "page_number", "figure", "aside_text",
        "list_item", "list", "page_footnote", "image",
    ];

    let mut result = String::new();
    let mut table_regions: Vec<TableRegion> = Vec::new();

    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
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
                i += 1;
                continue;
            }
        };

        // equation は \[ ... \] / \( ... \) の開閉が複数の生テキスト行に
        // またがって出力されることがあるため、閉じ括弧が現れるまで後続行を
        // 取り込んでから bbox プレフィックスを剥がして1つの数式として結合する。
        if elem_type == "equation" {
            let opening = if rest.starts_with('[') {
                rest.find(']').map(|idx| rest[idx + 1..].trim_start()).unwrap_or(rest)
            } else {
                rest.trim_start()
            };

            let is_closed = |s: &str| {
                let t = s.trim_end();
                t.ends_with("\\]") || t.ends_with("\\)")
            };

            let mut buf = String::from(opening);
            let mut j = i + 1;
            while !is_closed(&buf) && j < lines.len() {
                let next = lines[j].trim();
                if !next.is_empty() {
                    if !buf.is_empty() {
                        buf.push(' ');
                    }
                    buf.push_str(next);
                }
                j += 1;
            }

            if !buf.trim().is_empty() {
                result.push_str(buf.trim());
                result.push_str("\n\n");
            }
            i = j;
            continue;
        }

        // "[x1, y1, x2, y2]content" → content
        let content = if rest.starts_with('[') {
            rest.find(']').map(|i| rest[i + 1..].trim()).unwrap_or(rest)
        } else {
            rest.trim()
        };

        if content.is_empty() {
            i += 1;
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
            "text" | "caption" | "image_caption" | "list_item" | "list" | "page_footnote" => {
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
        i += 1;
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

    // フェンスもパイプ表も見つからない場合、生 HTML の <table> が返っていることがある。
    // そのまま本文に混入させず Markdown テーブルに変換してから返す。
    let trimmed = raw.trim();
    if trimmed.contains("<table") {
        return html_table_to_markdown(trimmed);
    }

    trimmed.to_string()
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

/// ページ画像を正規化トリミング範囲（0〜1）で切り出し、dest_dir に一時 PNG として保存する。
/// 座標変換は ui_preview.py の apply_crop と同じ仕様（クランプ→ピクセル丸め）。
fn crop_page_image_to_temp(src: &Path, crop: &CropRect, dest_dir: &Path) -> Result<PathBuf, String> {
    let img = image::open(src).map_err(|e| format!("画像読み込み失敗 {}: {e}", src.display()))?;
    let (w, h) = img.dimensions();

    let left = crop.left.clamp(0.0, 1.0);
    let top = crop.top.clamp(0.0, 1.0);
    let width = crop.width.clamp(0.0, 1.0 - left);
    let height = crop.height.clamp(0.0, 1.0 - top);

    let lpx = (left * w as f64).round() as u32;
    let tpx = (top * h as f64).round() as u32;
    let rpx = ((left + width) * w as f64).round() as u32;
    let bpx = ((top + height) * h as f64).round() as u32;

    if rpx <= lpx || bpx <= tpx {
        // 不正なトリミング範囲。元画像をそのまま一時ファイルにコピーして続行する。
        let dest = dest_dir.join(format!("crop_tmp_{}.png", uuid::Uuid::new_v4()));
        img.save(&dest).map_err(|e| format!("トリミング画像の保存失敗: {e}"))?;
        return Ok(dest);
    }

    let cropped = img.crop_imm(lpx, tpx, rpx - lpx, bpx - tpx);
    let dest = dest_dir.join(format!("crop_tmp_{}.png", uuid::Uuid::new_v4()));
    cropped
        .save(&dest)
        .map_err(|e| format!("トリミング画像の保存失敗: {e}"))?;
    Ok(dest)
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

/// Phase1（各ページの Unlimited OCR）で切り出し済みの表画像。Phase2 でまとめて
/// glm-ocr に渡すことで、モデル入れ替えをページごとではなく全体で1回に集約する。
struct PendingTable {
    md_path: PathBuf,
    placeholder: String,
    image_base64: String,
    html_fallback: String,
}

/// markdown 内の `<!--TABLE_REOCR_N-->` を処理する。表クロップ画像の切り出し・
/// エンコードは Ollama を呼ばない純粋な画像処理なので Phase1 のうちに済ませてしまい、
/// enable_table_reocr が ON の場合は PendingTable として持ち越す（プレースホルダは
/// markdown 内に残したまま）。OFF またはクロップ失敗時はその場で平坦テキストに解決する。
fn stage_table_placeholders(
    markdown: String,
    table_regions: Vec<TableRegion>,
    image_path: &Path,
    md_path: &Path,
    enable_table_reocr: bool,
) -> (String, Vec<PendingTable>) {
    if table_regions.is_empty() {
        return (markdown, Vec::new());
    }

    let base_img = if enable_table_reocr {
        image::open(image_path).ok()
    } else {
        None
    };

    let mut result = markdown;
    let mut pending = Vec::new();
    for (idx, region) in table_regions.iter().enumerate() {
        let placeholder = format!("<!--TABLE_REOCR_{idx}-->");
        let staged = base_img.as_ref().and_then(|img| {
            let cropped = crop_table_region(img, region.bbox);
            encode_table_crop_for_ocr(cropped).ok()
        });
        match staged {
            Some(image_base64) => pending.push(PendingTable {
                md_path: md_path.to_path_buf(),
                placeholder,
                image_base64,
                html_fallback: region.html.clone(),
            }),
            None => {
                result = result.replace(&placeholder, &html_table_to_markdown(&region.html));
            }
        }
    }
    (result, pending)
}

/// 保留しておいた表クロップ画像を glm-ocr で再OCR し、Markdown テーブルを返す。
async fn reocr_table_image(image_base64: &str, client: &OllamaClient) -> Result<String, String> {
    let raw = tokio::time::timeout(
        tokio::time::Duration::from_secs(300),
        client.chat_vision(TABLE_OCR_MODEL, "OCR", image_base64),
    )
    .await
    .map_err(|_| "glm-ocr 再OCR タイムアウト（300秒）".to_string())??;
    let raw = truncate_thought_leak(&raw);
    let raw = truncate_runaway_repetition(&raw);

    let table_md = extract_table_markdown(&raw);
    if table_md.is_empty() {
        return Err("glm-ocr 再OCR 結果が空でした".to_string());
    }
    Ok(table_md)
}

/// 表の再OCR を同時に投げるリクエスト数の上限。
///
/// 実機検証（Apple Silicon + OLLAMA_MLX=1 の Ollama）では、同一モデルへの
/// 同時リクエスト数を 3 にすると 1 に比べて大幅に遅くなった（6表で536秒 vs
/// 5表で83秒）。単一GPU上での同時推論がリソースの奪い合いになり、真の並列化
/// ではなくオーバーヘッドとして働いたためと考えられる。そのため既定値は 1
/// （実質逐次実行）とする。複数GPU環境や OLLAMA_NUM_PARALLEL を明示的に
/// 増やした環境で使う場合は、この値を上げる効果を再検証してから変更すること。
const TABLE_REOCR_CONCURRENCY: usize = 1;

/// Phase2: 保留していた表領域をまとめて glm-ocr で再OCR し、対応する page_*.md の
/// プレースホルダを置換する。
///
/// 表ごとに逐次 await していた旧実装は Ollama サーバーの並列処理能力を活かせず、
/// 表の数だけ待ち時間が線形に伸びる構造的なボトルネックだった。ここでは Ollama への
/// リクエストのみを `TABLE_REOCR_CONCURRENCY` 件まで同時実行し、ファイルの読み込み・
/// 置換・書き込みは全リクエストの結果が出揃ってから md_path ごとに1回だけ行う
/// （同一ファイルへの並行読み書きによる競合を避けるため、I/O は並列化しない）。
async fn resolve_pending_tables(
    pending_tables: &[PendingTable],
    client: &OllamaClient,
    total: u32,
    on_progress: Option<&ProgressCallback>,
) {
    let total_tables = pending_tables.len();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(TABLE_REOCR_CONCURRENCY));
    let mut join_set = tokio::task::JoinSet::new();

    for (idx, t) in pending_tables.iter().enumerate() {
        let sem = semaphore.clone();
        let client = client.clone();
        let image_base64 = t.image_base64.clone();
        join_set.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore は閉じられない");
            (idx, reocr_table_image(&image_base64, &client).await)
        });
    }

    let mut replacements: Vec<Option<String>> = vec![None; total_tables];
    let mut done = 0usize;
    let mut warned = false;

    while let Some(joined) = join_set.join_next().await {
        done += 1;
        if let Some(cb) = &on_progress {
            cb(total, total, &format!("表を再OCR中: {done}/{total_tables}"));
        }

        let (idx, result) = match joined {
            Ok(pair) => pair,
            Err(e) => {
                log::warn!("表の再OCR タスクが失敗（平坦テキストにフォールバック）: {e}");
                continue;
            }
        };

        replacements[idx] = Some(match result {
            Ok(md) => md,
            Err(e) => {
                log::warn!("表の再OCR失敗（平坦テキストにフォールバック）: {e}");
                if !warned {
                    if let Some(cb) = &on_progress {
                        cb(total, total, "表の再OCRに失敗したため平坦テキストで出力します");
                    }
                    warned = true;
                }
                html_table_to_markdown(&pending_tables[idx].html_fallback)
            }
        });
    }

    use std::collections::HashMap;
    let mut by_file: HashMap<&Path, Vec<usize>> = HashMap::new();
    for (idx, t) in pending_tables.iter().enumerate() {
        by_file.entry(t.md_path.as_path()).or_default().push(idx);
    }

    for (md_path, indices) in by_file {
        let Ok(mut content) = fs::read_to_string(md_path) else {
            continue;
        };
        for idx in indices {
            // タスクが JoinSet エラーで結果を返せなかった場合はプレースホルダを平坦テキストへ。
            let replacement = replacements[idx]
                .clone()
                .unwrap_or_else(|| html_table_to_markdown(&pending_tables[idx].html_fallback));
            content = content.replace(&pending_tables[idx].placeholder, &replacement);
        }
        let _ = fs::write(md_path, content);
    }
}

/// glm-ocr が利用できない場合、保留していた表を平坦テキストで解決する。
fn flatten_pending_tables(pending_tables: &[PendingTable]) -> Result<(), String> {
    use std::collections::HashMap;
    let mut by_file: HashMap<&Path, Vec<&PendingTable>> = HashMap::new();
    for t in pending_tables {
        by_file.entry(t.md_path.as_path()).or_default().push(t);
    }
    for (md_path, tables) in by_file {
        let mut content = fs::read_to_string(md_path)
            .map_err(|e| format!("{}の読み込み失敗: {e}", md_path.display()))?;
        for t in tables {
            content = content.replace(&t.placeholder, &html_table_to_markdown(&t.html_fallback));
        }
        fs::write(md_path, content)
            .map_err(|e| format!("{}の書き込み失敗: {e}", md_path.display()))?;
    }
    Ok(())
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
    /// 正規化トリミング範囲（left/top/width/height, 0〜1）。ページ画像に対して適用する。
    pub crop: Option<CropRect>,
    /// PDF に埋め込まれたテキストを（信頼できる場合に限り）そのまま使い、
    /// 該当ページの Ollama OCR 呼び出しをスキップする。
    pub use_embedded_text: bool,
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
            crop: None,
            use_embedded_text: false,
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
    let mut pending_tables: Vec<PendingTable> = Vec::new();
    let total_for_progress;

    if is_pdf_file(input_path) {
        // ページ数を取得してページ単位で変換・OCR する
        let total_pages = pdf_page_count(input_path, options.poppler_path.as_deref())?;
        let range_start = options.start_page.unwrap_or(1).max(1).min(total_pages);
        let range_end = options.end_page.unwrap_or(total_pages).max(range_start).min(total_pages);
        let total = range_end - range_start + 1;
        total_for_progress = total;

        // 埋め込みテキスト使用が有効な場合、ページ単位の処理に入る前に一括抽出しておく。
        // ページごとに毎回パースし直すと pdf-inspector のコストが人数分掛かってしまうため。
        let embedded_texts = if options.use_embedded_text {
            let input_path_owned = input_path.to_path_buf();
            match tokio::task::spawn_blocking(move || super::pdf_text::extract_page_texts(&input_path_owned)).await {
                Ok(Ok(texts)) => Some(texts),
                Ok(Err(e)) => {
                    log::warn!("埋め込みテキスト抽出に失敗したため通常の OCR にフォールバックします: {e}");
                    None
                }
                Err(e) => {
                    log::warn!("埋め込みテキスト抽出タスクが失敗したため通常の OCR にフォールバックします: {e}");
                    None
                }
            }
        } else {
            None
        };

        for (i, absolute_page) in (range_start..=range_end).enumerate() {
            let relative_page = (i + 1) as u32;

            let use_embedded = embedded_texts.as_ref().and_then(|e| {
                if e.needs_ocr.contains(&absolute_page) {
                    None
                } else {
                    e.texts.get(&absolute_page)
                }
            });

            if let Some(markdown) = use_embedded {
                // 埋め込みテキストが信頼できるページは Ollama OCR を呼ばず、抽出済みテキストを使う。
                // 図表抽出だけは必要ならページ画像を生成して従来通り実行する。
                if let Some(cb) = &on_progress {
                    cb(relative_page, total, &format!("埋め込みテキスト使用中: {relative_page}/{total}"));
                }

                let image_for_figures = if options.enable_figure {
                    if let Some(cb) = &on_progress {
                        cb(relative_page, total, &format!("PDF変換中: {relative_page}/{total}ページ"));
                    }
                    let raw_image_path = pdf_single_page_to_image(
                        input_path,
                        result_dir,
                        options.dpi,
                        options.poppler_path.as_deref(),
                        absolute_page,
                        relative_page,
                    )?;
                    Some(match &options.crop {
                        Some(crop) => {
                            let cropped = crop_page_image_to_temp(&raw_image_path, crop, result_dir)?;
                            let _ = fs::remove_file(&raw_image_path);
                            cropped
                        }
                        None => raw_image_path,
                    })
                } else {
                    None
                };

                let md_path = embedded_text_to_md(
                    markdown, image_for_figures.as_deref(), result_dir, relative_page, total, options, on_progress,
                ).await?;

                if let Some(image_path) = &image_for_figures {
                    let _ = fs::remove_file(image_path);
                }

                md_paths.push(md_path);

                if options.enable_rest && i + 1 < total as usize {
                    tokio::time::sleep(tokio::time::Duration::from_secs(options.rest_seconds)).await;
                }
                continue;
            }

            if let Some(cb) = &on_progress {
                cb(relative_page, total, &format!("PDF変換中: {relative_page}/{total}ページ"));
            }

            // 1ページだけ変換（CPU バーストを分散）
            let raw_image_path = pdf_single_page_to_image(
                input_path,
                result_dir,
                options.dpi,
                options.poppler_path.as_deref(),
                absolute_page,
                relative_page,
            )?;

            let image_path = match &options.crop {
                Some(crop) => {
                    let cropped = crop_page_image_to_temp(&raw_image_path, crop, result_dir)?;
                    let _ = fs::remove_file(&raw_image_path);
                    cropped
                }
                None => raw_image_path,
            };

            if let Some(cb) = &on_progress {
                cb(relative_page, total, &format!("OCR 処理中: {relative_page}/{total}"));
            }

            let (md_path, mut pending) = ocr_image_to_md(
                &image_path, result_dir, relative_page, total, options, &client, on_progress,
            ).await?;

            // ページ画像を即削除（メモリ・ディスク節約）
            let _ = fs::remove_file(&image_path);

            pending_tables.append(&mut pending);
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

    } else if is_image_file(input_path) {
        total_for_progress = 1;
        if let Some(cb) = &on_progress {
            cb(1, 1, "OCR 処理中: 1/1");
        }

        // 元ファイルは書き換えないため、トリミング時のみ一時ファイルを作る
        let cropped_path = match &options.crop {
            Some(crop) => Some(crop_page_image_to_temp(input_path, crop, result_dir)?),
            None => None,
        };
        let image_path = cropped_path.as_deref().unwrap_or(input_path);

        let (md_path, mut pending) = ocr_image_to_md(
            image_path, result_dir, 1, 1, options, &client, on_progress,
        ).await?;
        if let Some(p) = &cropped_path {
            let _ = fs::remove_file(p);
        }
        pending_tables.append(&mut pending);
        md_paths.push(md_path);

    } else {
        return Err(format!("未対応のファイル形式です: {}", input_path.display()));
    }

    // Phase2: 保留していた表領域をまとめて glm-ocr で再OCR する。
    // モデル入れ替えはページごとではなく、全体を通じて最大1回に集約される。
    if !pending_tables.is_empty() {
        let table_reocr_available = client.has_model(TABLE_OCR_MODEL).await.unwrap_or(false);
        if table_reocr_available {
            resolve_pending_tables(&pending_tables, &client, total_for_progress, on_progress).await;
        } else {
            if let Some(cb) = &on_progress {
                cb(total_for_progress, total_for_progress, "glm-ocr が見つからないため表は平坦テキストで出力します");
            }
            flatten_pending_tables(&pending_tables)?;
        }
    }

    if let Some(cb) = on_progress {
        cb(total_for_progress, total_for_progress, "OCR 完了");
    }

    Ok(md_paths)
}

/// 1枚の画像を OCR して page_NNN.md に保存する。表領域が見つかった場合は
/// PendingTable として返す（この時点では glm-ocr を呼ばない）。
async fn ocr_image_to_md(
    image_path: &Path,
    result_dir: &Path,
    page_num: u32,
    total: u32,
    options: &OcrOptions,
    client: &OllamaClient,
    on_progress: Option<&ProgressCallback>,
) -> Result<(PathBuf, Vec<PendingTable>), String> {
    let image_base64 = encode_image_for_ocr(image_path)?;

    let prompt = ocr_prompt_for(&options.ocr_model);
    let raw_ocr = client
        .chat_vision(&options.ocr_model, prompt, &image_base64)
        .await?;
    let raw_ocr = truncate_thought_leak(&raw_ocr);
    let raw_ocr = truncate_runaway_repetition(&raw_ocr);

    let md_path = result_dir.join(format!("page_{page_num:03}.md"));

    let (markdown, pending_tables) = if is_unlimited_ocr_format(&raw_ocr) {
        let (md, table_regions) = unlimited_ocr_to_markdown(&raw_ocr);
        let (md, pending) = stage_table_placeholders(
            md, table_regions, image_path, &md_path, options.enable_table_reocr,
        );
        (sanitize_math_delimiters(&md), pending)
    } else {
        (raw_ocr, Vec::new())
    };

    fs::write(&md_path, &markdown)
        .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;

    maybe_extract_figures(image_path, result_dir, &md_path, page_num, total, options, on_progress).await;

    Ok((md_path, pending_tables))
}

/// `enable_figure` が有効な場合に限り、ページ画像から図表を検出して md_path に追記する。
/// OCR 経路・埋め込みテキスト経路の両方から呼ばれる共通処理。
async fn maybe_extract_figures(
    image_path: &Path,
    result_dir: &Path,
    md_path: &Path,
    page_num: u32,
    total: u32,
    options: &OcrOptions,
    on_progress: Option<&ProgressCallback>,
) {
    if !options.enable_figure {
        return;
    }
    let (Some(py_bin), Some(script)) = (&options.python_bin, &options.detect_figures_script) else {
        return;
    };
    if !script.exists() {
        return;
    }

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
            append_figure_links(md_path, &fig_paths);
        }
        Ok(Ok(Err(e))) => log::warn!("Page {page_num}: 図表抽出失敗（続行）: {e}"),
        Err(_) => log::warn!("Page {page_num}: 図表抽出タイムアウト（スキップ）"),
        _ => {}
    }
}

/// pdf-inspector が抽出したページ Markdown をそのまま page_###.md として書き出す。
/// Ollama OCR は呼ばない。図表抽出用のページ画像が渡された場合のみ、それを使って
/// 図表検出（YOLOv8x）を実行する。
async fn embedded_text_to_md(
    markdown: &str,
    image_path: Option<&Path>,
    result_dir: &Path,
    page_num: u32,
    total: u32,
    options: &OcrOptions,
    on_progress: Option<&ProgressCallback>,
) -> Result<PathBuf, String> {
    let md_path = result_dir.join(format!("page_{page_num:03}.md"));
    fs::write(&md_path, markdown)
        .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;

    if let Some(image_path) = image_path {
        maybe_extract_figures(image_path, result_dir, &md_path, page_num, total, options, on_progress).await;
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
    fn html_table_to_markdown_preserves_multiple_rows() {
        let html = "<table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>";
        let result = html_table_to_markdown(html);
        assert_eq!(result, "| A | B |\n| --- | --- |\n| C | D |");
    }

    #[test]
    fn html_table_to_markdown_pads_ragged_rows_to_max_column_count() {
        let html = "<table><tr><td>A</td><td>B</td></tr><tr><td>C</td></tr></table>";
        let result = html_table_to_markdown(html);
        assert_eq!(result, "| A | B |\n| --- | --- |\n| C |  |");
    }

    #[test]
    fn html_table_to_markdown_keeps_empty_cells_to_preserve_column_alignment() {
        let html = "<table><tr><td>A</td><td></td><td>C</td></tr></table>";
        let result = html_table_to_markdown(html);
        assert_eq!(result, "| A |  | C |\n| --- | --- | --- |");
    }

    #[test]
    fn sanitize_math_delimiters_converts_to_dollar_syntax() {
        let text = "本文 \\( \\frac{5}{9} \\) の後に\n\n\\[ (0.4 \\times 0.6) = 0.24 \\]\n\n続き";
        let result = sanitize_math_delimiters(text);
        assert!(result.contains("$ \\frac{5}{9} $"));
        assert!(result.contains("$$ (0.4 \\times 0.6) = 0.24 $$"));
        assert!(!result.contains("\\("));
        assert!(!result.contains("\\["));
    }

    #[test]
    fn unlimited_ocr_to_markdown_joins_multiline_equation_and_strips_bbox() {
        let raw = "equation [190, 639, 863, 660]\\[\n\n(0. 4 \\times 0. 6) = 0. 2 4\n\n\\]\n\ntext [1, 2, 3, 4]続きの本文";
        let (md, regions) = unlimited_ocr_to_markdown(raw);
        assert!(regions.is_empty());
        assert!(!md.contains("equation ["));
        assert!(!md.contains("639"));
        assert!(md.contains("\\[ (0. 4 \\times 0. 6) = 0. 2 4 \\]"));
        assert!(md.contains("続きの本文"));
    }

    #[test]
    fn unlimited_ocr_to_markdown_handles_single_line_equation() {
        let raw = "equation [1, 2, 3, 4]\\( x = 1 \\)";
        let (md, _regions) = unlimited_ocr_to_markdown(raw);
        assert_eq!(md.trim(), "\\( x = 1 \\)");
    }

    #[test]
    fn truncate_runaway_repetition_cuts_exact_repeated_short_lines() {
        let raw = "本文の内容\n\n```\n```\n```\n```\n```\n```";
        let result = truncate_runaway_repetition(raw);
        assert_eq!(result.trim(), "本文の内容");
    }

    #[test]
    fn truncate_runaway_repetition_cuts_near_duplicate_degenerating_lines() {
        let raw = "見出し\n\ncontent诸葛亮王着的言訣 ApacheNullable(nullableない)?\ncontent诸葛亮王着的言置 ApacheNullablenullableない!)\ncontent诸葛亮王着的言詩apacheNullablenullableない?)\ncontent诸葛亮王着的言詩apacheNullablenullableない!)";
        let result = truncate_runaway_repetition(raw);
        assert_eq!(result.trim(), "見出し");
    }

    #[test]
    fn truncate_runaway_repetition_keeps_normal_text_untouched() {
        let raw = "第1段落。\n\n第2段落。\n\n第3段落。";
        let result = truncate_runaway_repetition(raw);
        assert_eq!(result, raw);
    }

    #[test]
    fn truncate_thought_leak_cuts_at_first_marker() {
        let raw = "B：小誌では，小謡家の成型申告中しないでも人知れず落している。\n\nOkay, I'm ready.\n```\n\nFinal transcription:\n\nB：小誌では、...";
        let result = truncate_thought_leak(raw);
        assert_eq!(
            result,
            "B：小誌では，小謡家の成型申告中しないでも人知れず落している。"
        );
    }

    #[test]
    fn truncate_thought_leak_keeps_normal_text_untouched() {
        let raw = "第1段落。\n\n第2段落。";
        let result = truncate_thought_leak(raw);
        assert_eq!(result, raw);
    }

    #[test]
    fn stage_table_placeholders_defers_to_pending_when_enabled() {
        let md = "本文\n\n<!--TABLE_REOCR_0-->\n\n続き".to_string();
        let regions = vec![TableRegion {
            bbox: (0, 0, 10, 10),
            html: "<table><td>A</td></table>".to_string(),
        }];
        let img_path = Path::new("this_file_does_not_exist.png");
        let md_path = Path::new("page_001.md");
        let (result, pending) =
            stage_table_placeholders(md, regions, img_path, md_path, true);
        // 画像が開けないので pending には積まれず、その場で平坦テキストに解決される
        assert!(pending.is_empty());
        assert!(!result.contains("TABLE_REOCR"));
        assert!(result.contains('A'));
    }

    #[test]
    fn stage_table_placeholders_flattens_immediately_when_disabled() {
        let md = "本文\n\n<!--TABLE_REOCR_0-->".to_string();
        let regions = vec![TableRegion {
            bbox: (0, 0, 10, 10),
            html: "<table><td>A</td><td>B</td></table>".to_string(),
        }];
        let img_path = Path::new("irrelevant.png");
        let md_path = Path::new("page_001.md");
        let (result, pending) =
            stage_table_placeholders(md, regions, img_path, md_path, false);
        assert!(pending.is_empty());
        assert!(result.contains("| A | B |"));
    }

    #[test]
    fn unlimited_ocr_to_markdown_keeps_list_and_page_footnote_as_body_text() {
        let raw = "list [137, 295, 941, 548]A：本文の内容\n\npage_footnote [120, 749, 411, 770]脚注の内容";
        let (md, _regions) = unlimited_ocr_to_markdown(raw);
        assert!(!md.contains("list ["));
        assert!(!md.contains("page_footnote ["));
        assert!(md.contains("A：本文の内容"));
        assert!(md.contains("脚注の内容"));
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
    fn extract_table_markdown_converts_raw_html_fallback_to_markdown() {
        let raw = r#"<table border="1"><tr><td></td><td>要旨把握</td><td>内容把握</td></tr></table>"#;
        let result = extract_table_markdown(raw);
        assert!(!result.contains("<table"));
        assert!(result.contains("要旨把握"));
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

    /// equation/repeat_penalty 修正の動作確認用の使い捨て統合テスト。
    /// Ollama 起動中 + Unlimited OCR + glm-ocr が必要。
    /// REOCR_PDF に PDF パス、REOCR_OUT に出力先ディレクトリを指定して実行する:
    /// REOCR_PDF=/path/to.pdf REOCR_OUT=/path/to/out cargo test --lib reocr_pdf_manual -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn reocr_pdf_manual() {
        let pdf = std::env::var("REOCR_PDF").expect("REOCR_PDF に PDF パスを指定してください");
        let pdf = std::path::PathBuf::from(pdf);
        assert!(pdf.exists(), "PDF が存在しません: {pdf:?}");

        let out_dir = std::env::var("REOCR_OUT").expect("REOCR_OUT に出力先ディレクトリを指定してください");
        let out_dir = std::path::PathBuf::from(out_dir);
        std::fs::create_dir_all(&out_dir).unwrap();

        let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string();

        let enable_table_reocr = std::env::var("REOCR_TABLE").as_deref() != Ok("0");
        let use_embedded_text = std::env::var("REOCR_EMBEDDED_TEXT").as_deref() == Ok("1");
        let start_page = std::env::var("REOCR_START").ok().and_then(|s| s.parse().ok());
        let end_page = std::env::var("REOCR_END").ok().and_then(|s| s.parse().ok());
        let options = OcrOptions {
            enable_figure: false,
            enable_table_reocr,
            use_embedded_text,
            start_page,
            end_page,
            ..Default::default()
        };

        let progress: ProgressCallback = Box::new(|current, total, msg| {
            println!("[reocr] {current}/{total}: {msg}");
        });

        let outputs = run_ocr_pipeline(&pdf, &out_dir, &options, Some(&progress))
            .await
            .unwrap_or_else(|e| panic!("pipeline 失敗: {e}"));
        assert!(!outputs.is_empty(), "出力ファイルなし");

        let merged = crate::markdown::merge_page_markdowns(&out_dir, &stem, true)
            .unwrap_or_else(|e| panic!("merge 失敗: {e}"));
        println!("merged markdown: {}", merged.display());
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
