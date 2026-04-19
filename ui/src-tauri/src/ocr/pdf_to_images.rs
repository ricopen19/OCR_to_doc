use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

/// PDF をページ画像に変換する。
/// 現在は Poppler の pdftoppm を subprocess で呼び出す方式。
/// 将来的に pdfium-render に置き換え可能。
/// PDF をページ画像に変換する（ページ範囲指定対応）。
/// start_page / end_page は 1-indexed。None で全ページ。
pub fn pdf_to_page_images_range(
    pdf_path: &Path,
    output_dir: &Path,
    dpi: u32,
    poppler_path: Option<&Path>,
    start_page: Option<u32>,
    end_page: Option<u32>,
) -> Result<Vec<PathBuf>, String> {
    let images_dir = output_dir.join("page_images");
    fs::create_dir_all(&images_dir)
        .map_err(|e| format!("page_images ディレクトリ作成失敗: {e}"))?;

    let pdftoppm = resolve_pdftoppm(poppler_path)?;

    let mut cmd = Command::new(&pdftoppm);
    cmd.arg("-png")
        .arg("-r")
        .arg(dpi.to_string());
    if let Some(s) = start_page {
        cmd.arg("-f").arg(s.to_string());
    }
    if let Some(e) = end_page {
        cmd.arg("-l").arg(e.to_string());
    }
    cmd.arg(pdf_path.to_str().unwrap_or_default())
        .arg(images_dir.join("page").to_str().unwrap_or_default());

    let status = cmd.status()
        .map_err(|e| format!("pdftoppm 実行失敗: {e}"))?;

    if !status.success() {
        return Err(format!("pdftoppm がエラーで終了 (code: {:?})", status.code()));
    }

    // pdftoppm の出力ファイルを収集してソート
    let mut page_images: Vec<PathBuf> = fs::read_dir(&images_dir)
        .map_err(|e| format!("page_images 読み込み失敗: {e}"))?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    page_images.sort();

    if page_images.is_empty() {
        return Err("pdftoppm がページ画像を生成しませんでした".to_string());
    }

    Ok(page_images)
}

/// pdftoppm のパスを解決する
fn resolve_pdftoppm(poppler_path: Option<&Path>) -> Result<PathBuf, String> {
    // 明示的に指定されたパス
    if let Some(p) = poppler_path {
        let candidate = p.join("pdftoppm");
        if candidate.exists() {
            return Ok(candidate);
        }
        let candidate_exe = p.join("pdftoppm.exe");
        if candidate_exe.exists() {
            return Ok(candidate_exe);
        }
    }

    // macOS: Homebrew
    #[cfg(target_os = "macos")]
    {
        for path in &[
            "/opt/homebrew/opt/poppler/bin/pdftoppm",
            "/usr/local/opt/poppler/bin/pdftoppm",
        ] {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    // PATH から探す
    if let Ok(output) = Command::new("which").arg("pdftoppm").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err("pdftoppm が見つかりません。Poppler をインストールしてください。".to_string())
}

/// 画像ファイルかどうか判定
pub fn is_image_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff" | "bmp" | "heic" | "heif")
    )
}

/// PDF ファイルかどうか判定
pub fn is_pdf_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("pdf")
    )
}
