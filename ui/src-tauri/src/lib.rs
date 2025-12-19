use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_plugin_dialog;
use uuid::Uuid;

#[derive(Default)]
struct AppState {
    jobs: Mutex<HashMap<String, JobInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JobInfo {
    status: JobStatus,
    progress: f32,
    log: Vec<String>,
    outputs: Vec<String>,
    preview: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum JobStatus {
    Idle,
    Running,
    Done,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunOptions {
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    image_as_pdf: bool,
    #[serde(default)]
    enable_figure: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunJobResponse {
    job_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressResponse {
    status: JobStatus,
    progress: f32,
    log: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultResponse {
    outputs: Vec<String>,
    preview: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    image_as_pdf: bool,
    #[serde(default)]
    enable_figure: bool,
    #[serde(default)]
    output_root: Option<String>,
}


#[tauri::command]
fn run_job(
    paths: Vec<String>,
    options: Option<RunOptions>,
    state: State<Arc<AppState>>,
) -> Result<RunJobResponse, String> {
    if paths.is_empty() {
        return Err("no input files".into());
    }

    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let dispatcher = project_root.join("dispatcher.py");
    if !dispatcher.exists() {
        return Err(format!(
            "dispatcher.py not found at {}",
            dispatcher.display()
        ));
    }

    let python_bin = resolve_python_bin(&project_root);

    let job_id = Uuid::new_v4().to_string();
    {
        let mut jobs = state
            .jobs
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?;
        jobs.insert(
            job_id.clone(),
            JobInfo {
                status: JobStatus::Running,
                progress: 0.0,
                log: vec!["job started".into()],
                outputs: vec![],
                preview: None,
                error: None,
            },
        );
    }

    let state_arc: Arc<AppState> = state.inner().clone();
    let dispatcher_path = dispatcher.clone();
    let (formats, image_as_pdf, enable_figure) = match options {
        Some(o) => (o.formats, o.image_as_pdf, o.enable_figure),
        None => (vec!["md".into()], false, true),
    };
    let python_bin_cloned = python_bin.clone();
    let project_root_cloned = project_root.clone();
    let paths_cloned = paths.clone();
    let job_id_cloned = job_id.clone();

    thread::spawn(move || {
        let mut outputs = Vec::new();
        let paths_len = paths_cloned.len();
        for (idx, p) in paths_cloned.iter().enumerate() {
            let mut cmd = Command::new(&python_bin_cloned);
            // Force unbuffered output for Python
            cmd.arg("-u"); 

            cmd.arg(&dispatcher_path).arg(p);
            if !formats.is_empty() {
                cmd.arg("--formats");
                for fmt in &formats {
                    cmd.arg(fmt);
                }
            }
            if image_as_pdf {
                cmd.arg("--image-as-pdf");
            }
            if enable_figure {
                cmd.arg("--figure");
            } else {
                cmd.arg("--no-figure");
            }
            cmd.current_dir(&project_root_cloned);
            
            // Pipe output to read in real-time
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let log_line = format!("spawn: {:?}", cmd);
            if let Ok(mut jobs) = state_arc.jobs.lock() {
                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                    job.log.push(log_line.clone());
                    // Start of this file processing
                    let base_progress = (idx as f32) / paths_len as f32 * 100.0;
                    job.progress = base_progress.min(99.0);
                }
            }

            match cmd.spawn() {
                Ok(mut child) => {
                    let stdout = child.stdout.take().expect("failed to get stdout");
                    let stderr = child.stderr.take().expect("failed to get stderr");

                    // Clone state for threads
                    let state_out = state_arc.clone();
                    let job_id_out = job_id_cloned.clone();
                    
                    // Stdout reader thread
                    let stdout_handle = thread::spawn(move || {
                        use std::io::{BufRead, BufReader};
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            if let Ok(l) = line {
                                if let Ok(mut jobs) = state_out.jobs.lock() {
                                    if let Some(job) = jobs.get_mut(&job_id_out) {
                                        job.log.push(l.clone());
                                        
                                        // Calculate current file's progress chunk
                                        let file_start = (idx as f32) / paths_len as f32 * 100.0;
                                        let file_end = ((idx as f32) + 1.0) / paths_len as f32 * 100.0;
                                        let _file_duration = file_end - file_start;

                                        // Progress heuristics
                                        let current_progress = if l.contains("画像を OCR ルートへ委譲") || l.contains("PDF を OCR ルートへ委譲") {
                                            file_start + (file_end - file_start) * 0.2
                                        } else if l.contains("Converting to docx") {
                                            // docx conversion is the last heavy step
                                            if l.contains("[dispatcher]") {
                                                 file_start + (file_end - file_start) * 0.8
                                            } else {
                                                 // from export_docx script? maybe 0.4 mid way?
                                                 // Let's stick to dispatcher logs mainly
                                                 job.progress // keep current
                                            }
                                        } else if l.contains("Word ファイルを出力しました") {
                                            file_start + (file_end - file_start) * 0.95
                                        } else {
                                            job.progress
                                        };
                                        
                                        if current_progress > job.progress {
                                            job.progress = current_progress.min(99.0);
                                        }
                                    }
                                }
                            }
                        }
                    });

                    // Stderr reader thread
                    let state_err = state_arc.clone();
                    let job_id_err = job_id_cloned.clone();
                    let stderr_handle = thread::spawn(move || {
                        use std::io::{BufRead, BufReader};
                        let reader = BufReader::new(stderr);
                        for line in reader.lines() {
                            if let Ok(l) = line {
                                if let Ok(mut jobs) = state_err.jobs.lock() {
                                    if let Some(job) = jobs.get_mut(&job_id_err) {
                                        job.log.push(format!("[err] {}", l));
                                    }
                                }
                            }
                        }
                    });

                    // Wait for finish
                    let status = child.wait();
                    stdout_handle.join().unwrap_or(());
                    stderr_handle.join().unwrap_or(());

                    match status {
                        Ok(s) if s.success() => {
                             if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                                    job.progress = ((idx as f32 + 1.0) / paths_len as f32 * 100.0).min(100.0);
                                }
                            }
                            outputs.push(p.clone());
                        }
                        Ok(_) => {
                            if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                                    job.status = JobStatus::Error;
                                    job.error = Some("dispatcher failed (non-zero exit code)".into());
                                }
                            }
                            return;
                        }
                        Err(e) => {
                             if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                                    job.status = JobStatus::Error;
                                    job.error = Some(format!("failed to spawn python: {e}"));
                                }
                            }
                            return;
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_cloned) {
                            job.status = JobStatus::Error;
                            job.error = Some(format!("failed to spawn python: {e}"));
                        }
                    }
                    return;
                }
            }
        }

        // set done
        if let Ok(mut jobs) = state_arc.jobs.lock() {
            if let Some(job) = jobs.get_mut(&job_id_cloned) {
                job.status = JobStatus::Done;
                job.progress = 100.0;
                let output_files =
                    collect_output_files(&project_root_cloned, &paths_cloned, &formats);
                job.outputs = output_files
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();

                // Markdownプレビュー: 最初に見つかった md を読む
                if let Some(md_path) = output_files
                    .iter()
                    .find(|p| p.extension().map(|e| e == "md").unwrap_or(false))
                {
                    if let Ok(content) = fs::read_to_string(md_path) {
                        job.preview = Some(content);
                    } else {
                        job.preview = Some(format!(
                            "failed to read markdown preview: {}",
                            md_path.display()
                        ));
                    }
                } else {
                    job.preview = Some(format!(
                        "Converted markdown for: {} (md preview not found)",
                        outputs.join(", ")
                    ));
                }
            }
        }
    });

    Ok(RunJobResponse { job_id })
}

/// Resolve python binary path with priority:
/// 1) env PYTHON_BIN
/// 2) project_root/.venv/bin/python (Unix) or Scripts/python.exe (Windows)
/// 3) "python"
fn resolve_python_bin(project_root: &std::path::Path) -> String {
    if let Ok(bin) = std::env::var("PYTHON_BIN") {
        if !bin.is_empty() {
            return bin;
        }
    }

    // resources/.venv (配布用に同梱する場合)
    #[cfg(target_os = "windows")]
    let res_venv = project_root
        .join("resources")
        .join(".venv")
        .join("Scripts")
        .join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let res_venv = project_root
        .join("resources")
        .join(".venv")
        .join("bin")
        .join("python");
    if res_venv.exists() {
        return res_venv.to_string_lossy().to_string();
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

#[tauri::command]
fn get_progress(job_id: String, state: State<Arc<AppState>>) -> Result<ProgressResponse, String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(job) = jobs.get(&job_id) {
        return Ok(ProgressResponse {
            status: job.status.clone(),
            progress: job.progress,
            log: job.log.clone(),
            error: job.error.clone(),
        });
    }
    Err("job not found".into())
}

/// 入力パスに応じて出力候補を探す
fn collect_output_files(
    project_root: &std::path::Path,
    inputs: &[String],
    formats: &[String],
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for input in inputs {
        let input_path = PathBuf::from(input);
        let stem_owned = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();
        let stem = stem_owned.as_str();

        // result/<stem> ディレクトリ内
        let result_dir = project_root.join("result").join(stem);
        if result_dir.exists() {
            for fmt in formats {
                let candidate = result_dir.join(format!("{}_merged.{}", stem, fmt));
                if candidate.exists() {
                    found.push(candidate);
                    continue;
                }
                let candidate2 = result_dir.join(format!("{}.{}", stem, fmt));
                if candidate2.exists() {
                    found.push(candidate2);
                    continue;
                }
            }
        }

        // ルート直下に <stem>_merged.<fmt> / <stem>.<fmt>
        for fmt in formats {
            let c1 = project_root.join(format!("{}_merged.{}", stem, fmt));
            if c1.exists() {
                found.push(c1);
                continue;
            }
            let c2 = project_root.join(format!("{}.{}", stem, fmt));
            if c2.exists() {
                found.push(c2);
                continue;
            }
        }
    }
    found
}

/// Walk ancestors from exe_dir to find dispatcher.py; return its parent (project root)
fn resolve_project_root(exe_dir: &std::path::Path) -> Option<PathBuf> {
    for anc in exe_dir.ancestors() {
        let candidate = anc.join("dispatcher.py");
        if candidate.exists() {
            return Some(anc.to_path_buf());
        }
    }
    None
}

#[tauri::command]
fn get_result(job_id: String, state: State<Arc<AppState>>) -> Result<ResultResponse, String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(job) = jobs.get(&job_id) {
        return Ok(ResultResponse {
            outputs: job.outputs.clone(),
            preview: job.preview.clone(),
        });
    }
    Err("job not found".into())
}

#[tauri::command]
fn save_file(
    job_id: String,
    filename: String,
    dest_path: String,
    state: State<Arc<AppState>>,
) -> Result<(), String> {
    println!(
        "[save_file] called with job_id={}, filename={}, dest_path={}",
        job_id, filename, dest_path
    );
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;

    if let Some(job) = jobs.get(&job_id) {
        println!("[save_file] job found. outputs={:?}", job.outputs);
        // filename が outputs に含まれているか確認 (セキュリティ対策)
        if !job.outputs.contains(&filename) {
            println!("[save_file] filename not in outputs");
            return Err(format!("file not found in job outputs: {}", filename));
        }

        // 元ファイルを探す
        let exe_dir =
            std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
        let project_root =
            resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

        let mut source_path = None;

        // 1. result ディレクトリ内を探索
        let result_dir = project_root.join("result");
        if result_dir.exists() {
            if let Ok(entries) = fs::read_dir(&result_dir) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join(&filename);
                    if candidate.exists() {
                        source_path = Some(candidate);
                        break;
                    }
                }
            }
        }

        if source_path.is_none() {
            // プロジェクトルート直下
            let candidate = project_root.join(&filename);
            if candidate.exists() {
                source_path = Some(candidate);
            }
        }

        if let Some(src) = source_path {
            fs::copy(&src, &dest_path).map_err(|e| format!("failed to copy file: {e}"))?;
            return Ok(());
        } else {
            return Err(format!("source file not found: {}", filename));
        }
    }
    Err("job not found".into())
}

#[tauri::command]
fn load_settings() -> Result<AppSettings, String> {
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&exe_dir).unwrap_or_else(|| PathBuf::from("."));
    
    // Ensure configs directory exists
    let config_dir = project_root.join("configs");
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    let settings_path = config_dir.join("settings.json");
    if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        let settings: AppSettings = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        Ok(settings)
    } else {
        // Return defaults
        Ok(AppSettings {
            formats: vec!["md".into()],
            image_as_pdf: false,
            enable_figure: true,
            output_root: None,
        })
    }
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<(), String> {
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&exe_dir).unwrap_or_else(|| PathBuf::from("."));
    
    let config_dir = project_root.join("configs");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    }

    let settings_path = config_dir.join("settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    
    fs::write(settings_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            run_job,
            get_progress,
            get_result,
            save_file,
            load_settings,
            save_settings
        ])
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
