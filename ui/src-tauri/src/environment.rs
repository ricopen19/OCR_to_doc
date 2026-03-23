use std::path::PathBuf;

use crate::job::EnvironmentStatus;
use crate::paths::{
    resolve_output_root, resolve_python_bin, resolve_resource_roots, resolve_project_root,
};
use crate::load_settings_from_disk;

pub async fn check_environment() -> Result<EnvironmentStatus, String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let settings = load_settings_from_disk(&project_root).ok();
    let result_root = resolve_output_root(&project_root, settings.as_ref());
    let result_dir_found = result_root.exists();
    let python_bin = resolve_python_bin(&project_root);
    let python_path = if python_bin == "python" {
        None
    } else {
        Some(python_bin.clone())
    };
    let python_found = python_path
        .as_ref()
        .map(|p| PathBuf::from(p).is_file())
        .unwrap_or(false);

    let resource_roots = resolve_resource_roots(&project_root);
    let resource_roots_display = resource_roots
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let mut dispatcher_candidates = resource_roots
        .iter()
        .map(|root| root.join("py").join("dispatcher.py"))
        .collect::<Vec<_>>();
    dispatcher_candidates.push(project_root.join("dispatcher.py"));
    let dispatcher_path = dispatcher_candidates.into_iter().find(|p| p.is_file());
    let dispatcher_found = dispatcher_path.is_some();

    let mut poppler_candidates = Vec::new();
    for root in &resource_roots {
        let base = root.join("py").join("poppler");
        #[cfg(target_os = "windows")]
        {
            poppler_candidates.push(base.join("Library").join("bin"));
            poppler_candidates.push(base.join("win").join("bin"));
        }
        #[cfg(target_os = "macos")]
        {
            poppler_candidates.push(base.join("macos").join("bin"));
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let os_name = std::env::consts::OS;
            poppler_candidates.push(base.join(os_name).join("bin"));
        }
    }
    let poppler_path = poppler_candidates.into_iter().find(|dir| {
        if !dir.is_dir() {
            return false;
        }
        #[cfg(target_os = "windows")]
        let pdfinfo = dir.join("pdfinfo.exe");
        #[cfg(not(target_os = "windows"))]
        let pdfinfo = dir.join("pdfinfo");
        #[cfg(target_os = "windows")]
        let pdftoppm = dir.join("pdftoppm.exe");
        #[cfg(not(target_os = "windows"))]
        let pdftoppm = dir.join("pdftoppm");
        pdfinfo.exists() || pdftoppm.exists()
    });
    let poppler_found = poppler_path.is_some();

    // Ollama チェック
    let ollama_client = crate::ollama::client::OllamaClient::new();
    let ocr_model_name = "glm-ocr".to_string();
    let ollama_running = ollama_client.health_check().await.unwrap_or(false);
    let ocr_model_ready = if ollama_running {
        ollama_client
            .has_model(&ocr_model_name)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    Ok(EnvironmentStatus {
        project_root: project_root.to_string_lossy().to_string(),
        os: std::env::consts::OS.to_string(),
        dispatcher_found,
        dispatcher_path: dispatcher_path.map(|p| p.to_string_lossy().to_string()),
        result_dir_found,
        result_root: result_root.to_string_lossy().to_string(),
        python_bin,
        python_found,
        python_path,
        poppler_found,
        poppler_path: poppler_path.map(|p| p.to_string_lossy().to_string()),
        resource_roots: resource_roots_display,
        ollama_running,
        ocr_model_ready,
        ocr_model_name,
    })
}
