use std::fs;
use std::path::{Path, PathBuf};

use image::GenericImageView;
use serde::Deserialize;

use crate::ollama::client::OllamaClient;

const FIGURE_DETECT_PROMPT: &str = r#"この画像内の図、イラスト、写真、グラフの位置を特定してください。
テキストや表は含めないでください。
各図について以下の JSON 配列形式のみで回答してください（説明文は不要です）:
[{"label": "図の簡潔な説明", "bbox": [x1, y1, x2, y2]}]
座標は画像の左上を原点とし、ピクセル単位で指定してください。
図が見つからない場合は空配列 [] を返してください。"#;

#[derive(Debug, Deserialize)]
struct FigureDetection {
    label: String,
    bbox: [u32; 4], // [x1, y1, x2, y2]
}

/// ページ画像から図表を検出し、切り出して figures/ に保存する。
/// Ollama の VLM を使って bbox を検出する方式。
pub async fn extract_figures(
    page_image_path: &Path,
    result_dir: &Path,
    page_number: u32,
    model: &str,
) -> Result<Vec<PathBuf>, String> {
    let client = OllamaClient::new();
    let figures_dir = result_dir.join("figures");
    fs::create_dir_all(&figures_dir)
        .map_err(|e| format!("figures/ 作成失敗: {e}"))?;

    // 画像を base64 エンコード
    let image_bytes = fs::read(page_image_path)
        .map_err(|e| format!("画像読み込み失敗: {e}"))?;
    let image_base64 = base64::engine::general_purpose::STANDARD
        .encode(&image_bytes);

    // VLM に図の位置を問い合わせ
    let response = client
        .chat_vision(model, FIGURE_DETECT_PROMPT, &image_base64)
        .await?;

    // JSON をパース
    let detections = parse_figure_detections(&response)?;
    if detections.is_empty() {
        return Ok(Vec::new());
    }

    // 元画像を読み込み
    let img = image::open(page_image_path)
        .map_err(|e| format!("画像デコード失敗: {e}"))?;
    let (img_width, img_height) = img.dimensions();

    let mut saved_paths = Vec::new();

    for (i, detection) in detections.iter().enumerate() {
        let [x1, y1, x2, y2] = detection.bbox;

        // bbox のバリデーション
        let x1 = x1.min(img_width);
        let y1 = y1.min(img_height);
        let x2 = x2.min(img_width).max(x1 + 1);
        let y2 = y2.min(img_height).max(y1 + 1);
        let w = x2 - x1;
        let h = y2 - y1;

        // 小さすぎる領域はスキップ（アイコン等）
        let area = (w as u64) * (h as u64);
        if area < 2500 {
            continue;
        }

        // 切り出し
        let cropped = img.crop_imm(x1, y1, w, h);
        let fig_name = format!("fig_page{page_number:03}_{:02}.png", i + 1);
        let fig_path = figures_dir.join(&fig_name);
        cropped
            .save(&fig_path)
            .map_err(|e| format!("figure 保存失敗: {e}"))?;

        saved_paths.push(fig_path);
    }

    Ok(saved_paths)
}

/// VLM の応答から FigureDetection のリストをパースする
fn parse_figure_detections(response: &str) -> Result<Vec<FigureDetection>, String> {
    // レスポンスから JSON 配列部分を抽出
    let trimmed = response.trim();

    // まず直接パースを試みる
    if let Ok(detections) = serde_json::from_str::<Vec<FigureDetection>>(trimmed) {
        return Ok(detections);
    }

    // ```json ... ``` ブロックから抽出
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            let json_str = &trimmed[start..=end];
            if let Ok(detections) = serde_json::from_str::<Vec<FigureDetection>>(json_str) {
                return Ok(detections);
            }
        }
    }

    // パース失敗 = 図が見つからなかったと解釈
    Ok(Vec::new())
}

use base64::Engine;
