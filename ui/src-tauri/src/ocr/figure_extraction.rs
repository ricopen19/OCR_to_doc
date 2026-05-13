use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// ページ画像から図表を検出し、切り出して figures/ に保存する。
/// YOLOv8x-DocLayNet を Python (detect_figures.py) で実行する方式。
pub fn extract_figures(
    page_image_path: &Path,
    result_dir: &Path,
    page_number: u32,
    python_bin: &str,
    script_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    if !script_path.exists() {
        return Err(format!(
            "detect_figures.py が見つかりません: {}",
            script_path.display()
        ));
    }

    let mut cmd = if let Some(uv) = crate::paths::find_uv() {
        let mut c = Command::new(uv);
        crate::paths::apply_python_env(&mut c);
        c.arg("run")
            .arg("--no-project")
            .arg("--with").arg("ultralytics,huggingface_hub,Pillow");
        c
    } else {
        let mut c = Command::new(python_bin);
        crate::paths::apply_python_env(&mut c);
        c.arg("-u");
        c
    };
    cmd.arg(script_path)
        .arg(page_image_path)
        .arg(result_dir)
        .arg(page_number.to_string());

    let output = cmd
        .output()
        .map_err(|e| format!("detect_figures.py 実行失敗: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("detect_figures.py エラー: {stderr}"));
    }

    // figures/ ディレクトリから該当ページの図を収集
    let figures_dir = result_dir.join("figures");
    if !figures_dir.exists() {
        return Ok(Vec::new());
    }

    let prefix = format!("fig_page{page_number:03}_");
    let mut paths: Vec<PathBuf> = fs::read_dir(&figures_dir)
        .map_err(|e| format!("figures/ 読み込み失敗: {e}"))?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            if name.starts_with(&prefix) && name.ends_with(".png") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    paths.sort();

    Ok(paths)
}
