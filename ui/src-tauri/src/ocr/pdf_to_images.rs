use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

/// PDF の総ページ数を pdfinfo で取得する。
pub fn pdf_page_count(pdf_path: &Path, poppler_path: Option<&Path>) -> Result<u32, String> {
    let pdfinfo = resolve_poppler_tool("pdfinfo", poppler_path)?;

    let output = Command::new(&pdfinfo)
        .arg(pdf_path)
        .output()
        .map_err(|e| format!("pdfinfo 実行失敗: {e}"))?;

    if !output.status.success() {
        return Err("pdfinfo がエラーで終了".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            if let Ok(n) = rest.trim().parse::<u32>() {
                return Ok(n);
            }
        }
    }

    Err("ページ数を取得できませんでした".to_string())
}

/// PDF の 1 ページだけを画像に変換する。
/// absolute_page: PDF 内の実際のページ番号（1-indexed）
/// relative_page: 出力ファイルの連番（page_001.png 等）
pub fn pdf_single_page_to_image(
    pdf_path: &Path,
    output_dir: &Path,
    dpi: u32,
    poppler_path: Option<&Path>,
    absolute_page: u32,
    relative_page: u32,
) -> Result<PathBuf, String> {
    let images_dir = output_dir.join("page_images");
    fs::create_dir_all(&images_dir)
        .map_err(|e| format!("page_images ディレクトリ作成失敗: {e}"))?;

    let pdftoppm = resolve_poppler_tool("pdftoppm", poppler_path)?;

    // ページごとにユニークなプレフィックスを使い、出力ファイルを確実に特定する
    let prefix = images_dir.join(format!("tmp_{absolute_page:06}"));

    let mut cmd = Command::new(&pdftoppm);
    cmd.arg("-png")
        .arg("-r").arg(dpi.to_string())
        .arg("-f").arg(absolute_page.to_string())
        .arg("-l").arg(absolute_page.to_string())
        .arg(pdf_path)
        .arg(&prefix);

    let status = cmd.status()
        .map_err(|e| format!("pdftoppm 実行失敗: {e}"))?;

    if !status.success() {
        return Err(format!("pdftoppm がエラーで終了 (ページ {absolute_page})"));
    }

    // pdftoppm は prefix-N.png を生成する（N はゼロ埋め）
    let prefix_name = format!("tmp_{absolute_page:06}");
    let png = fs::read_dir(&images_dir)
        .map_err(|e| format!("page_images 読み込み失敗: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("png")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(&prefix_name))
                    .unwrap_or(false)
        })
        .ok_or_else(|| format!("pdftoppm 出力画像が見つかりません (ページ {absolute_page})"))?;

    // 連番ファイル名にリネーム
    let final_path = images_dir.join(format!("page_{relative_page:03}.png"));
    fs::rename(&png, &final_path)
        .map_err(|e| format!("画像ファイルのリネーム失敗: {e}"))?;

    Ok(final_path)
}

/// Poppler ツール（pdftoppm / pdfinfo 等）のパスを解決する。
fn resolve_poppler_tool(name: &str, poppler_path: Option<&Path>) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let exe = format!("{name}.exe");
    #[cfg(not(target_os = "windows"))]
    let exe = name.to_string();

    if let Some(p) = poppler_path {
        let c = p.join(&exe);
        if c.exists() {
            return Ok(c);
        }
    }

    #[cfg(target_os = "macos")]
    for base in &[
        "/opt/homebrew/opt/poppler/bin",
        "/usr/local/opt/poppler/bin",
    ] {
        let p = PathBuf::from(base).join(&exe);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(output) = Command::new("which").arg(name).output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Ok(PathBuf::from(s));
            }
        }
    }

    Err(format!("{name} が見つかりません。Poppler をインストールしてください。"))
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
