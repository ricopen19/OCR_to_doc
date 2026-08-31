use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::GenericImageView;
use regex::Regex;

use crate::job::CropRect;
use crate::ollama::engine::{BackendConfig, OcrBackend};
use super::pdf_to_images::{
    is_pdf_file, is_image_file,
    pdf_page_count, pdf_single_page_to_image,
};

/// 既定の OCR モデル。以前は速度優先で Unlimited-OCR-GGUF（本文）と glm-ocr（表の
/// 再OCR）を使い分けていたが、Unlimited OCR は本文生成でも既知の暴走生成
/// （反復ハルシネーション）を起こすことが実機検証で判明したため撤去し、
/// 過去に安定運用できていた glm-ocr 一本構成に戻した。
/// エンジン選択（ADR-018）後も、設定でモデルを指定しなかった場合の既定値。
pub(crate) use crate::ollama::engine::DEFAULT_OCR_MODEL as OCR_MODEL;

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

/// `\textcircled{N}` は原資料の丸数字（①②③...）を glm-ocr が LaTeX コマンドとして
/// 誤符号化したもの。KaTeX 等の軽量 Markdown レンダラは `\textcircled` に対応して
/// おらず、`$ ... $` で囲んでも数式として描画されない（丸数字という情報が失われる）
/// ため、対応する Unicode 丸数字文字に変換し数式扱いをやめる。Unicode の丸数字は
/// 1〜20 のみ連続したコードポイントを持つため、その範囲外は変換せず元のまま残す。
fn convert_circled_numbers(text: &str) -> String {
    const CIRCLED_DIGITS: [char; 20] = [
        '①', '②', '③', '④', '⑤', '⑥', '⑦', '⑧', '⑨', '⑩',
        '⑪', '⑫', '⑬', '⑭', '⑮', '⑯', '⑰', '⑱', '⑲', '⑳',
    ];
    let re = Regex::new(r"\$?\\textcircled\{(\d{1,2})\}\$?").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        match caps[1].parse::<usize>() {
            Ok(n) if (1..=20).contains(&n) => CIRCLED_DIGITS[n - 1].to_string(),
            _ => caps[0].to_string(),
        }
    })
    .into_owned()
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
/// （同一・酷似した行、または数行単位のブロックが延々と繰り返される）を検知し、
/// 繰り返しが始まった位置で応答を打ち切る安全網。
///
/// ページ全体の生テキストにも、表とは無関係に数行単位のブロックが延々と
/// 繰り返される暴走が実際に発生することを実機データ（`1周間SPI_模擬4.pdf`）で
/// 確認した（選択肢テキストの3行ブロックが ``` フェンスに包まれながら
/// 数十〜100回以上反復し、途中から無関係な中国語の歴史文献名を生成するなど
/// 完全に暴走する事例）。表の行反復には別途 `find_repeating_rows_cut`
/// （間隔の不揃いに対応した行単位の対応追跡）を用意しているが、ページ全体の
/// 生テキストはそこを経由しないため、こちらのブロック検知も必要。
///
/// 単一行の反復（block_len=1）だけでなく、複数行が1セットになって周期的に
/// 反復するパターン（block_len=2..=MAX_REPEAT_BLOCK_LEN）も検知する。各ブロック長
/// ごとに、行の先頭位置をずらした全オフセットを試すことで、反復の開始位置が
/// ブロック境界と揃っていないケースも見逃さない。ブロック内の類似判定は、結合
/// テキスト全体のbigram類似度ではなく対応する位置の行を1行ずつ比較し、全行が
/// 一致する場合のみ「反復」と判定する（語彙の狭い短い行同士が偶然噛み合って
/// 誤検知するのを防ぐため）。出現回数の閾値はブロック長に応じて滑らかに下げる
/// （短い行は偶然の一致が起きやすいため4回出現を要求するが、長いブロックほど
/// 少ない出現回数でも安全に暴走とみなせる）。
fn truncate_runaway_repetition(raw: &str) -> String {
    const MAX_REPEAT_BLOCK_LEN: usize = 24;
    const MIN_REPEATED_LINES: usize = 8;

    fn required_occurrences_for(block_len: usize) -> usize {
        let by_content_volume = MIN_REPEATED_LINES.div_ceil(block_len);
        by_content_volume.clamp(2, 4)
    }

    let lines: Vec<&str> = raw.lines().collect();
    let non_empty: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let trimmed = l.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some((i, trimmed))
            }
        })
        .collect();

    let cut = (1..=MAX_REPEAT_BLOCK_LEN)
        .filter_map(|block_len| {
            let repeat_threshold = required_occurrences_for(block_len).saturating_sub(1).max(1);
            detect_block_repeat_cut(&non_empty, block_len, repeat_threshold)
        })
        .min();

    match cut {
        Some(cut_idx) => lines[..cut_idx].join("\n").trim_end().to_string(),
        None => raw.to_string(),
    }
}

/// 2つの同じ長さのブロックが「反復」とみなせるかを、対応する位置の行同士を
/// 1行ずつ比較して判定する（結合テキスト全体のbigram類似度は、語彙が狭く
/// 共通しやすい短い行の集まりで無関係なブロック同士でも誤検知するため使わない）。
fn blocks_similar(a: &[&str], b: &[&str]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| lines_similar(x, y))
}

/// `non_empty`（空行を除いた (元の行番号, 行内容) の列）上で、長さ `block_len` の
/// 連続ブロックが直前のブロックと類似する状態が `repeat_threshold` 回続いたら、
/// 反復が始まった元の行番号を返す（打ち切り位置。この行を含めて捨てる）。
fn detect_block_repeat_cut(
    non_empty: &[(usize, &str)],
    block_len: usize,
    repeat_threshold: usize,
) -> Option<usize> {
    let n = non_empty.len();
    if block_len == 0 || n < block_len * 2 {
        return None;
    }

    let block_slice = |start: usize| -> Vec<&str> {
        non_empty[start..start + block_len].iter().map(|&(_, s)| s).collect()
    };

    // オフセットによって最初に見つかる切り捨て位置が変わりうる（境界のずれで
    // 反復の検知が遅れることがある）ため、全オフセットを試して最も早い
    // 切り捨て位置を採用する。
    let mut earliest_cut: Option<usize> = None;

    for offset in 0..block_len {
        let mut prev: Option<Vec<&str>> = None;
        let mut prev_start_orig_idx = 0usize;
        let mut run_len = 0usize;
        let mut run_start_orig_idx = 0usize;
        let mut i = offset;
        while i + block_len <= n {
            let cur = block_slice(i);
            let cur_start_orig_idx = non_empty[i].0;
            if let Some(p) = &prev {
                if blocks_similar(p, &cur) {
                    if run_len == 0 {
                        run_start_orig_idx = prev_start_orig_idx;
                    }
                    run_len += 1;
                    if run_len >= repeat_threshold {
                        earliest_cut = Some(earliest_cut.map_or(run_start_orig_idx, |c| c.min(run_start_orig_idx)));
                        break;
                    }
                } else {
                    run_len = 0;
                }
            }
            prev = Some(cur);
            prev_start_orig_idx = cur_start_orig_idx;
            i += block_len;
        }
    }

    earliest_cut
}

/// glm-ocr はページ全体のOCR結果を一度出力した後、``` フェンスに包んで内容を
/// もう一度出力し直すことがある（表クロップの再OCRで確認されていた「プレーン
/// 出力→```table フェンス付き再出力」の癖が、ページ全体の出力でも起きる）。
/// 実機データ（`1周間SPI_模擬4.pdf`）で、フェンス内の再出力は単なる重複ではなく、
/// 誤字・意味不明な単語の混入・フェンスが閉じないまま出力が終わるなど、後半に
/// いくほど劣化する「劣化する再生成」であることを確認した。そのためフェンス内の
/// 内容がそれ以前の本文と大部分一致する場合は、健全な前半（フェンス開始位置より
/// 前）を残し、フェンス以降をまるごと捨てる。
///
/// 既知の限界: フェンスを手がかりに検出しているため、フェンスを伴わない同様の
/// 重複再生成が起きた場合は検知できない。現時点ではその実例は確認していない。
fn truncate_duplicate_reemission(raw: &str) -> String {
    const MATCH_RATIO_THRESHOLD: f64 = 0.5;

    let lines: Vec<&str> = raw.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if !line.trim().starts_with("```") {
            continue;
        }

        let before: Vec<&str> = lines[..i]
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if before.is_empty() {
            continue;
        }

        // フェンスの中身（閉じフェンスが見つからなければ末尾まで）を集める。
        let close_offset = lines[i + 1..]
            .iter()
            .position(|l| l.trim() == "```")
            .map(|p| i + 1 + p)
            .unwrap_or(lines.len());
        let fenced: Vec<&str> = lines[i + 1..close_offset]
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        if fenced.is_empty() {
            continue;
        }

        let matched = fenced
            .iter()
            .filter(|f| before.iter().any(|b| lines_similar(b, f)))
            .count();
        let ratio = matched as f64 / fenced.len() as f64;

        if ratio >= MATCH_RATIO_THRESHOLD {
            return lines[..i].join("\n").trim_end().to_string();
        }
    }

    raw.to_string()
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
    /// 正規化トリミング範囲（left/top/width/height, 0〜1）。ページ画像に対して適用する。
    pub crop: Option<CropRect>,
    /// PDF に埋め込まれたテキストを（信頼できる場合に限り）そのまま使い、
    /// 該当ページの OCR 呼び出しをスキップする。
    pub use_embedded_text: bool,
    /// OCR バックエンド（エンジンと接続先）。
    pub backend: BackendConfig,
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
            crop: None,
            use_embedded_text: false,
            backend: BackendConfig::ollama_default(),
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

    let client = OcrBackend::new(&options.backend);

    if !client.health_check().await? {
        return Err(client.not_running_hint());
    }
    client.ensure_model(&options.ocr_model).await?;

    let mut md_paths = Vec::new();
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

            let md_path = ocr_image_to_md(
                &image_path, result_dir, relative_page, total, options, &client, on_progress,
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

        let md_path = ocr_image_to_md(
            image_path, result_dir, 1, 1, options, &client, on_progress,
        ).await?;
        if let Some(p) = &cropped_path {
            let _ = fs::remove_file(p);
        }
        md_paths.push(md_path);

    } else {
        return Err(format!("未対応のファイル形式です: {}", input_path.display()));
    }

    if let Some(cb) = on_progress {
        cb(total_for_progress, total_for_progress, "OCR 完了");
    }

    Ok(md_paths)
}

/// 1枚の画像を OCR して page_NNN.md に保存する。表領域が見つかった場合は
/// glm-ocr が返した Markdown をそのまま page_NNN.md として書き出す。
async fn ocr_image_to_md(
    image_path: &Path,
    result_dir: &Path,
    page_num: u32,
    total: u32,
    options: &OcrOptions,
    client: &OcrBackend,
    on_progress: Option<&ProgressCallback>,
) -> Result<PathBuf, String> {
    let image_base64 = encode_image_for_ocr(image_path)?;

    let raw_ocr = client
        .chat_vision(&options.ocr_model, "OCR", &image_base64)
        .await?;
    let raw_ocr = truncate_thought_leak(&raw_ocr);
    let raw_ocr = truncate_runaway_repetition(&raw_ocr);
    let raw_ocr = truncate_duplicate_reemission(&raw_ocr);
    let markdown = sanitize_math_delimiters(&raw_ocr);
    let markdown = convert_circled_numbers(&markdown);

    let md_path = result_dir.join(format!("page_{page_num:03}.md"));
    fs::write(&md_path, &markdown)
        .map_err(|e| format!("page_{page_num:03}.md 書き込み失敗: {e}"))?;

    maybe_extract_figures(image_path, result_dir, &md_path, page_num, total, options, on_progress).await;

    Ok(md_path)
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
    fn sanitize_math_delimiters_converts_to_dollar_syntax() {
        let text = "本文 \\( \\frac{5}{9} \\) の後に\n\n\\[ (0.4 \\times 0.6) = 0.24 \\]\n\n続き";
        let result = sanitize_math_delimiters(text);
        assert!(result.contains("$ \\frac{5}{9} $"));
        assert!(result.contains("$$ (0.4 \\times 0.6) = 0.24 $$"));
        assert!(!result.contains("\\("));
        assert!(!result.contains("\\["));
    }

    #[test]
    fn convert_circled_numbers_replaces_textcircled_with_unicode() {
        let text = "確実にいえるのは$\\textcircled{1}$から$\\textcircled{3}$のどれか。D $\\textcircled{1}$$\\textcircled{2}$";
        let result = convert_circled_numbers(text);
        assert_eq!(result, "確実にいえるのは①から③のどれか。D ①②");
        assert!(!result.contains("textcircled"));
    }

    #[test]
    fn convert_circled_numbers_leaves_out_of_range_untouched() {
        let text = "$\\textcircled{25}$";
        let result = convert_circled_numbers(text);
        assert_eq!(result, text);
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
    fn truncate_runaway_repetition_cuts_non_table_block_runaway() {
        // 実機（1周間SPI_模擬4.pdf）で実際に発生した暴走生成の回帰テスト。
        // 表とは無関係な、ページ本文中の選択肢テキスト（3行1セット）が
        // ```フェンスに包まれながら周期的に繰り返される。
        let raw = "(1) ある2日間で、初日が雨で、次の日が雨でない確率はいくらか。\n\n\
A 0.16 B 0.36 C 0.56\n\
D 0.48 E 0.24 F 0.1\n\
G 0.2 H A～Gのいずれでもない\n\n\
```markdown\n\n\
A 0.16 B 0.36 C 0.56\n\
D 0.48 E 0.24 F 0.1\n\
G 0.2 H A〜Gのいずれでもない\n\
```\n\
```\n\
A 0.16 B 0.36 C 0.56\n\
D 0.48 E 0.24 F 0.1\n\
G 0.2 H A〜Gのいずれでもない\n\
```\n\
```\n\
A 0.16 B 0.36 C 0.56\n\
D 0.48 E 0.24 F 0.1\n\
G 0.2 H A〜Gのいずれでもない\n\
```";
        let result = truncate_runaway_repetition(raw);
        assert_eq!(
            result.matches("A 0.16 B 0.36 C 0.56").count(),
            1,
            "反復した2回目以降のブロックが残っている: {result}"
        );
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
    fn truncate_duplicate_reemission_cuts_at_fence_when_content_matches_earlier_text() {
        // 実機（1周間SPI_模擬4.pdf Page3）で確認した回帰テスト。ページ全体を
        // OCRした後、```markdown フェンスに包んで内容をもう一度出力し直す
        // （後半は誤字混入で劣化している）。
        let raw = "問題\n\n\
X、Y、Zの3人が集まった。Xが2000円の食事を、Yが1450円のスイーツを買った。\n\
A 550円 B 600円 C 650円\n\
```markdown\n\n\
問題\n\n\
X、Y、Zの3人が集まった。Xが2000円の食事を、Yが1450円のスイーツを見い買った。\n\
A 550円 B 600円 C 650円\n\
```";
        let result = truncate_duplicate_reemission(raw);
        assert_eq!(
            result.matches("問題").count(),
            1,
            "フェンス内の重複再生成が残っている: {result}"
        );
        assert!(!result.contains("```"));
    }

    #[test]
    fn truncate_duplicate_reemission_keeps_normal_fenced_content_untouched() {
        // フェンスの中身が本文と無関係な場合（一致率が低い）は切り捨てない。
        let raw = "本文の説明。\n\n```\nprint(\"hello\")\n```";
        let result = truncate_duplicate_reemission(raw);
        assert_eq!(result, raw);
    }

    /// 実機検証用の使い捨て統合テスト。Ollama 起動中 + glm-ocr が必要。
    /// REOCR_PDF に PDF パス、REOCR_OUT に出力先ディレクトリを指定して実行する:
    /// REOCR_PDF=/path/to.pdf REOCR_OUT=/path/to/out cargo test --lib reocr_pdf_manual -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn reocr_pdf_manual() {
        let _ = env_logger::builder().is_test(true).try_init();
        let pdf = std::env::var("REOCR_PDF").expect("REOCR_PDF に PDF パスを指定してください");
        let pdf = std::path::PathBuf::from(pdf);
        assert!(pdf.exists(), "PDF が存在しません: {pdf:?}");

        let out_dir = std::env::var("REOCR_OUT").expect("REOCR_OUT に出力先ディレクトリを指定してください");
        let out_dir = std::path::PathBuf::from(out_dir);
        std::fs::create_dir_all(&out_dir).unwrap();

        let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string();

        let use_embedded_text = std::env::var("REOCR_EMBEDDED_TEXT").as_deref() == Ok("1");
        let start_page = std::env::var("REOCR_START").ok().and_then(|s| s.parse().ok());
        let end_page = std::env::var("REOCR_END").ok().and_then(|s| s.parse().ok());
        let options = OcrOptions {
            enable_figure: false,
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

    /// 実機検証用の使い捨て統合テスト（llama.cpp / mlx-vlm 経路）。
    /// OpenAI 互換サーバーを起動しておき、実 PDF/画像で通し実行する。
    /// serde 往復（ContentPart 送信・ChatCompletionResponse / ModelsResponse 受信）と
    /// OcrBackend の分岐を実サーバーに対して検証する。
    ///
    /// LLAMACPP_MODEL=mlx-community/Qwen3-VL-8B-Instruct-4bit \
    ///   OCR_INPUT=/path/to.pdf OCR_OUT=/tmp/e2e \
    ///   cargo test --lib ocr_pipeline_llamacpp_manual -- --ignored --nocapture
    /// LLAMACPP_URL は省略時 http://127.0.0.1:8080。
    #[tokio::test]
    #[ignore]
    async fn ocr_pipeline_llamacpp_manual() {
        let _ = env_logger::builder().is_test(true).try_init();
        let url = std::env::var("LLAMACPP_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let model = std::env::var("LLAMACPP_MODEL")
            .expect("LLAMACPP_MODEL に起動中モデルの id を指定してください");
        let input = std::path::PathBuf::from(
            std::env::var("OCR_INPUT").expect("OCR_INPUT に PDF / 画像パスを指定してください"),
        );
        assert!(input.exists(), "入力が存在しません: {input:?}");
        let out_dir = std::path::PathBuf::from(
            std::env::var("OCR_OUT").expect("OCR_OUT に出力先ディレクトリを指定してください"),
        );
        std::fs::create_dir_all(&out_dir).unwrap();

        let cfg = BackendConfig::new(
            crate::ollama::engine::OcrEngine::LlamaCpp,
            Some(url.clone()),
            std::env::var("LLAMACPP_API_KEY").ok(),
        );

        // 1) /v1/models の serde 往復。起動中モデルが一覧に含まれること。
        let models = OcrBackend::new(&cfg)
            .list_models()
            .await
            .unwrap_or_else(|e| panic!("list_models 失敗: {e}"));
        println!("[e2e] /v1/models -> {models:?}");
        assert!(
            models.iter().any(|m| m == &model),
            "起動中モデル {model} が /v1/models に見当たらない"
        );

        // 2) パイプライン通し実行（画像エンコード → OpenAiClient → 後処理 → md 書き出し）
        let options = OcrOptions {
            ocr_model: model.clone(),
            enable_figure: false,
            start_page: std::env::var("OCR_START").ok().and_then(|s| s.parse().ok()),
            end_page: std::env::var("OCR_END").ok().and_then(|s| s.parse().ok()),
            backend: cfg,
            ..Default::default()
        };
        let progress: ProgressCallback = Box::new(|current, total, msg| {
            println!("[e2e] {current}/{total}: {msg}");
        });
        let outputs = run_ocr_pipeline(&input, &out_dir, &options, Some(&progress))
            .await
            .unwrap_or_else(|e| panic!("pipeline 失敗: {e}"));
        assert!(!outputs.is_empty(), "出力ファイルなし");

        let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string();
        let merged = crate::markdown::merge_page_markdowns(&out_dir, &stem, true)
            .unwrap_or_else(|e| panic!("merge 失敗: {e}"));
        let text = std::fs::read_to_string(&merged).unwrap();
        println!("[e2e] merged: {}\n---\n{}\n---", merged.display(), text);
        assert!(text.trim().len() > 10, "OCR 結果が空に近い");
    }
}
