use std::path::{Path, PathBuf};
use std::process::Command;

use crate::settings::AppSettings;

const APP_SUPPORT_DIR_NAME: &str = "ocr-to-doc";

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

pub fn resolve_python_entry(project_root: &Path, filename: &str) -> PathBuf {
    for root in resolve_resource_roots(project_root) {
        let res = root.join("py").join(filename);
        if res.exists() {
            return res;
        }
    }
    project_root.join(filename)
}

/// Resolve python binary path with priority:
/// 1) env PYTHON_BIN
/// 2) project_root/resources/python/python(.exe)
/// 3) project_root/resources/.venv/(Scripts|bin)/python(.exe)
/// 4) project_root/.venv/(Scripts|bin)/python(.exe)
/// 5) "python"
pub fn resolve_python_bin(project_root: &Path) -> String {
    if let Ok(bin) = std::env::var("PYTHON_BIN") {
        if !bin.is_empty() {
            return bin;
        }
    }

    for root in resolve_resource_roots(project_root) {
        #[cfg(target_os = "windows")]
        let res_python = root.join("python").join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let res_python = root.join("python").join("bin").join("python");
        if res_python.exists() {
            return res_python.to_string_lossy().to_string();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let res_python3 = root.join("python").join("bin").join("python3");
            if res_python3.exists() {
                return res_python3.to_string_lossy().to_string();
            }
        }
    }

    for root in resolve_resource_roots(project_root) {
        #[cfg(target_os = "windows")]
        let res_venv = root.join(".venv").join("Scripts").join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let res_venv = root.join(".venv").join("bin").join("python");
        if res_venv.exists() {
            return res_venv.to_string_lossy().to_string();
        }
    }

    #[cfg(target_os = "windows")]
    let venv = project_root
        .join(".venv")
        .join("Scripts")
        .join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let venv = project_root.join(".venv").join("bin").join("python");
    if venv.exists() {
        return venv.to_string_lossy().to_string();
    }
    "python".into()
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

pub fn resolve_resource_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let direct = project_root.join("resources");
    roots.push(direct);
    let up = project_root.join("_up_").join("resources");
    if up != roots[0] {
        roots.push(up);
    }
    roots
}

/// Walk ancestors from exe_dir to find dispatcher.py; return its parent (project root)
pub fn resolve_project_root(exe_dir: &Path) -> Option<PathBuf> {
    for anc in exe_dir.ancestors() {
        let res = anc.join("resources").join("py").join("dispatcher.py");
        let legacy = anc.join("dispatcher.py");
        let app_res = anc
            .join("Contents")
            .join("Resources")
            .join("resources")
            .join("py")
            .join("dispatcher.py");
        let app_res_up = anc
            .join("Contents")
            .join("Resources")
            .join("_up_")
            .join("resources")
            .join("py")
            .join("dispatcher.py");
        if app_res.exists() {
            return Some(anc.join("Contents").join("Resources"));
        }
        if app_res_up.exists() {
            return Some(anc.join("Contents").join("Resources"));
        }
        if res.exists() {
            return Some(anc.to_path_buf());
        }
        if legacy.exists() {
            return Some(anc.to_path_buf());
        }
    }
    None
}
