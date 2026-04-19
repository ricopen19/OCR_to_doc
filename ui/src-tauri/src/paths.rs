use std::path::{Path, PathBuf};
use std::process::Command;

use crate::settings::AppSettings;

const APP_SUPPORT_DIR_NAME: &str = "ocr-to-doc";
const SCRIPTS_PYTHON: &[&str] = &["scripts", "python"];
const BUNDLE_UP: &[&str] = &["_up_", "_up_"];

pub fn apply_python_env(cmd: &mut Command) {
    cmd.env("PYTHONUTF8", "1");
    cmd.env("PYTHONIOENCODING", "utf-8");
}

pub fn default_gpu_device() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "mps"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "cuda"
    }
}

fn join_segments(base: &Path, segs: &[&str]) -> PathBuf {
    let mut p = base.to_path_buf();
    for s in segs {
        p.push(s);
    }
    p
}

/// project_root 直下の scripts/python/ と、バンドル内の _up_/_up_/scripts/python/ の2箇所を候補として返す。
pub fn resolve_python_dir_candidates(project_root: &Path) -> Vec<PathBuf> {
    vec![
        join_segments(project_root, SCRIPTS_PYTHON),
        join_segments(&join_segments(project_root, BUNDLE_UP), SCRIPTS_PYTHON),
    ]
}

pub fn resolve_python_entry(project_root: &Path, filename: &str) -> PathBuf {
    for dir in resolve_python_dir_candidates(project_root) {
        let p = dir.join(filename);
        if p.exists() {
            return p;
        }
    }
    // fallback (caller はファイル存在チェックする想定)
    join_segments(project_root, SCRIPTS_PYTHON).join(filename)
}

/// Resolve python binary path with priority:
/// 1) env PYTHON_BIN
/// 2) project_root/.venv/(Scripts|bin)/python(.exe)
/// 3) 祖先の .venv（dev で exe が target/debug/ の場合）
/// 4) "python"
pub fn resolve_python_bin(project_root: &Path) -> String {
    if let Ok(bin) = std::env::var("PYTHON_BIN") {
        if !bin.is_empty() {
            return bin;
        }
    }

    if let Some(found) = find_venv_python(project_root) {
        return found;
    }
    for anc in project_root.ancestors().skip(1) {
        if let Some(found) = find_venv_python(anc) {
            return found;
        }
    }
    "python".into()
}

fn find_venv_python(root: &Path) -> Option<String> {
    #[cfg(target_os = "windows")]
    let candidate = root.join(".venv").join("Scripts").join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let candidate = root.join(".venv").join("bin").join("python");
    if candidate.exists() {
        return Some(candidate.to_string_lossy().to_string());
    }
    None
}

pub fn is_app_bundle_resource_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str());
    if name != Some("Resources") {
        return false;
    }
    let contents = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
    if contents != Some("Contents") {
        return false;
    }
    path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        == Some("app")
}

pub fn resolve_app_support_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_SUPPORT_DIR_NAME),
        );
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))?;
        return Some(PathBuf::from(base).join(APP_SUPPORT_DIR_NAME));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(base).join(APP_SUPPORT_DIR_NAME));
        }
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(APP_SUPPORT_DIR_NAME),
        );
    }
}

pub fn resolve_config_dir(project_root: &Path) -> PathBuf {
    if is_app_bundle_resource_dir(project_root) {
        if let Some(app_support) = resolve_app_support_dir() {
            return app_support.join("configs");
        }
    }
    project_root.join("configs")
}

pub fn expand_tilde_path(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

pub fn resolve_default_output_root(project_root: &Path) -> PathBuf {
    if is_app_bundle_resource_dir(project_root) {
        if let Some(app_support) = resolve_app_support_dir() {
            return app_support.join("result");
        }
    }
    project_root.join("result")
}

pub fn resolve_output_root(project_root: &Path, settings: Option<&AppSettings>) -> PathBuf {
    if let Some(s) = settings {
        if let Some(custom) = s.output_root.as_ref() {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return expand_tilde_path(trimmed);
            }
        }
    }
    resolve_default_output_root(project_root)
}

pub fn resolve_output_root_from_disk(project_root: &Path) -> PathBuf {
    let config_dir = resolve_config_dir(project_root);
    let settings = crate::settings::load_settings_from_disk(project_root, &config_dir).ok();
    resolve_output_root(project_root, settings.as_ref())
}

/// exe_dir の祖先を辿り、scripts/python/ が直接（または _up_/_up_/scripts/python/ として）存在する場所を返す。
/// 戻り値は "project_root"。dev ならリポジトリルート、macOS .app なら Contents/Resources。
pub fn resolve_project_root(exe_dir: &Path) -> Option<PathBuf> {
    for anc in exe_dir.ancestors() {
        // Dev: ancestor/scripts/python/
        if join_segments(anc, SCRIPTS_PYTHON).is_dir() {
            return Some(anc.to_path_buf());
        }
        // macOS .app: ancestor/Contents/Resources/_up_/_up_/scripts/python/
        let bundle_res = anc.join("Contents").join("Resources");
        let bundle_python = join_segments(&join_segments(&bundle_res, BUNDLE_UP), SCRIPTS_PYTHON);
        if bundle_python.is_dir() {
            return Some(bundle_res);
        }
    }
    None
}
