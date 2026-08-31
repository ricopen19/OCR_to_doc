mod ollama;
mod ocr;
mod markdown;
mod export;
mod settings;
mod job;
mod paths;
mod cli;
mod results;
mod environment;

use settings::AppSettings;
use job::{
    AppState, JobInfo, JobStatus, RunOptions, CropRect,
    RunJobResponse, ProgressResponse, ResultResponse, RecentResultEntry,
    EnvironmentStatus, PreviewResponse, PdfTextDetectionResponse,
};
use paths::{
    apply_python_env, find_uv, resolve_python_entry, resolve_python_bin,
    resolve_config_dir, resolve_output_root, resolve_output_root_from_disk,
    resolve_project_root, resolve_poppler_bin_dir,
};
use results::{
    find_output_path, open_path_with_default_app,
    validate_result_dir_name, canonicalize_dir, pick_best_file_in_dir,
};

use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
};
use tauri::{Manager, State};
use tauri_plugin_dialog;
use uuid::Uuid;

pub use cli::run_cli_if_requested;

pub(crate) fn load_settings_from_disk(project_root: &std::path::Path) -> Result<AppSettings, String> {
    let config_dir = resolve_config_dir(project_root);
    settings::load_settings_from_disk(project_root, &config_dir)
}

fn apply_window_settings(app: &tauri::AppHandle, project_root: &std::path::Path) {
    let settings = load_settings_from_disk(project_root).ok();
    let width = settings
        .as_ref()
        .and_then(|s| s.window_width)
        .unwrap_or(1200)
        .max(720);
    let height = settings
        .as_ref()
        .and_then(|s| s.window_height)
        .unwrap_or(760)
        .max(540);

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize::new(
            720.0, 540.0,
        ))));
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            width as f64,
            height as f64,
        )));
    }
}

/// Ollama (GLM-OCR) ベースの OCR ジョブ実行コマンド。
#[tauri::command]
async fn run_job_ollama(
    paths: Vec<String>,
    options: Option<RunOptions>,
    state: State<'_, Arc<AppState>>,
) -> Result<RunJobResponse, String> {
    if paths.is_empty() {
        return Err("入力ファイルがありません".into());
    }

    let exe_dir = std::env::current_exe().map_err(|e| format!("exe path 取得失敗: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("project root 解決失敗")?;
    let settings = load_settings_from_disk(&project_root).ok();
    let output_root = resolve_output_root(&project_root, settings.as_ref());
    fs::create_dir_all(&output_root)
        .map_err(|e| format!("出力ディレクトリ作成失敗: {e}"))?;

    let opts = options.unwrap_or_else(|| RunOptions {
        formats: vec!["md".into()],
        enable_figure: false,
        docx_engine: None,
        enable_rest: false,
        rest_seconds: None,
        pdf_dpi: None,
        excel_mode: None,
        excel_meta_sheet: None,
        file_options: None,
        use_embedded_text: false,
        ocr_engine: None,
        ocr_model: None,
        llama_base_url: None,
        llama_api_key: None,
        llama_model: None,
    });

    use crate::ollama::engine::OcrEngine;
    let engine = OcrEngine::parse(opts.ocr_engine.as_deref());

    // エンジンごとにモデル欄が別（Ollama: ocr_model / llama.cpp: llama_model）。
    // llama.cpp はサーバーが指定名のモデルをロードしようとするため、未選択のまま
    // 実行させると HF fetch に走って失敗する。ここで弾く。
    let ocr_model = match engine {
        OcrEngine::LlamaCpp => {
            let m = opts.llama_model.as_deref().unwrap_or("").trim().to_string();
            if m.is_empty() {
                return Err(
                    "llama.cpp エンジンではモデルを選択してください（設定画面で「再取得」→モデルを選択）。"
                        .into(),
                );
            }
            m
        }
        OcrEngine::Ollama => crate::ollama::engine::resolve_ocr_model(opts.ocr_model.clone()),
    };

    let job_id = Uuid::new_v4().to_string();
    {
        let mut jobs = state.jobs.lock().map_err(|e| format!("lock: {e}"))?;
        jobs.insert(
            job_id.clone(),
            JobInfo {
                status: JobStatus::Running,
                progress: 0.0,
                log: vec!["Ollama OCR ジョブ開始".into()],
                outputs: vec![],
                output_paths: vec![],
                preview: None,
                error: None,
                current_message: Some("Ollama に接続中...".into()),
                page_current: None,
                page_total: None,
                eta_seconds: None,
            },
        );
    }

    let state_arc: Arc<AppState> = state.inner().clone();
    let job_id_clone = job_id.clone();
    let dpi = opts.pdf_dpi.unwrap_or(300);
    let enable_figure = opts.enable_figure;
    let enable_rest = opts.enable_rest;
    let rest_seconds = opts.rest_seconds.unwrap_or(10) as u64;
    let formats = opts.formats.clone();
    let file_options = opts.file_options.clone();
    let docx_engine = opts.docx_engine.clone();
    let excel_mode = opts.excel_mode.clone();
    let excel_meta_sheet = opts.excel_meta_sheet;
    let use_embedded_text = opts.use_embedded_text;
    let backend_cfg = crate::ollama::engine::BackendConfig::new(
        engine,
        opts.llama_base_url.clone(),
        opts.llama_api_key.clone(),
    );
    let python_bin = resolve_python_bin(&project_root);
    let project_root_clone = project_root.clone();

    // バックグラウンドスレッドで OCR 実行
    tokio::spawn(async move {
        let total_files = paths.len();

        for (file_idx, input_path_str) in paths.iter().enumerate() {
            let input_path = std::path::PathBuf::from(input_path_str);
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");

            // file_options からページ範囲・トリミング範囲を取得
            let (start_page, end_page, crop) = file_options
                .as_ref()
                .and_then(|m| m.get(input_path_str))
                .map(|fo| (fo.start, fo.end, fo.crop.clone()))
                .unwrap_or((None, None, None));

            // ページ範囲指定があればディレクトリ名にサフィックスを付加
            let dir_name = match (start_page, end_page) {
                (Some(s), Some(e)) => format!("{stem}_p{s}-{e}"),
                (Some(s), None) => format!("{stem}_p{s}-"),
                (None, Some(e)) => format!("{stem}_p1-{e}"),
                (None, None) => stem.to_string(),
            };
            let result_dir = output_root.join(&dir_name);

            let detect_script = resolve_python_entry(&project_root_clone, "detect_figures.py");
            let ocr_options = ocr::pipeline::OcrOptions {
                ocr_model: ocr_model.clone(),
                dpi,
                poppler_path: resolve_poppler_bin_dir(&project_root_clone),
                enable_figure,
                python_bin: Some(python_bin.clone()),
                detect_figures_script: Some(detect_script),
                start_page,
                end_page,
                enable_rest,
                rest_seconds,
                crop,
                use_embedded_text,
                backend: backend_cfg.clone(),
            };

            // 進捗コールバック
            let state_cb = state_arc.clone();
            let job_id_cb = job_id_clone.clone();
            let file_start = (file_idx as f32 / total_files as f32) * 100.0;
            let file_range = 100.0 / total_files as f32;
            let on_progress: ocr::pipeline::ProgressCallback =
                Box::new(move |current, total, msg| {
                    if let Ok(mut jobs) = state_cb.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_cb) {
                            let ocr_ratio = current as f32 / total.max(1) as f32;
                            job.progress = file_start + file_range * 0.90 * ocr_ratio;
                            job.current_message = Some(msg.to_string());
                            job.page_current = Some(current);
                            job.page_total = Some(total);
                        }
                    }
                });

            // OCR 実行
            match ocr::pipeline::run_ocr_pipeline(
                &input_path,
                &result_dir,
                &ocr_options,
                Some(&on_progress),
            )
            .await
            {
                Ok(_md_paths) => {
                    // Markdown マージ
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_clone) {
                            job.progress = file_start + file_range * 0.92;
                            job.current_message = Some("Markdown マージ中...".into());
                        }
                    }

                    let _ = markdown::merge_page_markdowns(&result_dir, &dir_name, true);

                    let merged_md = result_dir.join(format!("{dir_name}_merged.md"));

                    // docx エクスポート（Python 呼び出し）
                    if formats.iter().any(|f| f == "docx") {
                        if let Ok(mut jobs) = state_arc.jobs.lock() {
                            if let Some(job) = jobs.get_mut(&job_id_clone) {
                                job.progress = file_start + file_range * 0.94;
                                job.current_message =
                                    Some("docx 変換中...".into());
                            }
                        }

                        if merged_md.exists() {
                            let export_script =
                                resolve_python_entry(&project_root_clone, "export_docx.py");
                            if export_script.exists() {
                                let mut cmd = if let Some(uv) = find_uv() {
                                    let mut c = Command::new(uv);
                                    apply_python_env(&mut c);
                                    c.arg("run").arg("--no-project").arg("--with").arg("python-docx");
                                    c
                                } else {
                                    let mut c = Command::new(&python_bin);
                                    apply_python_env(&mut c);
                                    c
                                };
                                cmd.arg(&export_script).arg(&merged_md);
                                if docx_engine.as_deref() == Some("pandoc") {
                                    cmd.arg("--use-pandoc");
                                }
                                match cmd.status() {
                                    Ok(s) if s.success() => {}
                                    Ok(s) => {
                                        if let Ok(mut jobs) = state_arc.jobs.lock() {
                                            if let Some(job) = jobs.get_mut(&job_id_clone) {
                                                job.log.push(format!(
                                                    "[warn] export_docx.py 終了コード: {}",
                                                    s.code().unwrap_or(-1)
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        if let Ok(mut jobs) = state_arc.jobs.lock() {
                                            if let Some(job) = jobs.get_mut(&job_id_clone) {
                                                job.log.push(format!(
                                                    "[error] export_docx.py 実行失敗: {e}"
                                                ));
                                            }
                                        }
                                    }
                                }
                            } else if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_clone) {
                                    job.log.push(format!(
                                        "[warn] export_docx.py が見つかりません: {}",
                                        export_script.display()
                                    ));
                                }
                            }
                        }
                    }

                    // xlsx / csv エクスポート（Markdown テーブル → xlsx + csv）
                    let want_xlsx = formats.iter().any(|f| f == "xlsx");
                    let want_csv = formats.iter().any(|f| f == "csv");
                    if (want_xlsx || want_csv) && merged_md.exists() {
                        if let Ok(mut jobs) = state_arc.jobs.lock() {
                            if let Some(job) = jobs.get_mut(&job_id_clone) {
                                job.progress = file_start + file_range * 0.97;
                                job.current_message =
                                    Some("xlsx/csv 変換中...".into());
                            }
                        }

                        let xlsx_path = result_dir.join(format!("{dir_name}.xlsx"));
                        let export_script =
                            resolve_python_entry(&project_root_clone, "export_excel_poc.py");
                        if export_script.exists() {
                            let mut cmd = Command::new(&python_bin);
                            apply_python_env(&mut cmd);
                            cmd.arg(&export_script)
                                .arg(&merged_md)
                                .arg(&xlsx_path)
                                .arg("--format").arg("markdown");
                            if want_csv {
                                cmd.arg("--csv-dir").arg(&result_dir);
                            }
                            if let Some(mode) = &excel_mode {
                                cmd.arg("--excel-mode").arg(mode);
                            }
                            match excel_meta_sheet {
                                Some(true) => { cmd.arg("--meta"); }
                                Some(false) => { cmd.arg("--no-meta"); }
                                None => {}
                            }
                            let _ = cmd.status();
                            // xlsx が不要なら削除
                            if !want_xlsx {
                                let _ = fs::remove_file(&xlsx_path);
                            }
                        }
                    }

                    // 出力ファイル収集
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_clone) {
                            let mut outputs = Vec::new();
                            let mut output_paths = Vec::new();
                            if let Ok(entries) = fs::read_dir(&result_dir) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                        if (name.ends_with("_merged.md") && formats.iter().any(|f| f == "md"))
                                            || name.ends_with("_merged.docx")
                                            || name.ends_with(".xlsx")
                                            || name.ends_with(".csv")
                                        {
                                            outputs.push(name.to_string());
                                            output_paths
                                                .push(path.to_string_lossy().to_string());
                                        }
                                    }
                                }
                            }

                            // プレビュー（merged.md の内容）
                            let merged_md_preview = result_dir.join(format!("{dir_name}_merged.md"));
                            let preview = fs::read_to_string(&merged_md_preview).ok();

                            job.outputs = outputs;
                            job.output_paths = output_paths;
                            job.preview = preview;
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_clone) {
                            job.status = JobStatus::Error;
                            job.error = Some(e);
                            job.current_message = Some("エラーが発生しました".into());
                        }
                    }
                    return;
                }
            }
        }

        // 完了
        if let Ok(mut jobs) = state_arc.jobs.lock() {
            if let Some(job) = jobs.get_mut(&job_id_clone) {
                if job.status != JobStatus::Error {
                    job.status = JobStatus::Done;
                    job.progress = 100.0;
                    job.current_message = Some("完了".into());
                }
            }
        }
    });

    Ok(RunJobResponse { job_id })
}

fn sanitize_extension(extension: Option<String>) -> String {
    let fallback = "png".to_string();
    let ext = extension.unwrap_or_default().to_lowercase();
    let trimmed = ext.trim().trim_start_matches('.');
    let clean: String = trimmed.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if clean.is_empty() {
        fallback
    } else {
        clean
    }
}

#[tauri::command]
fn save_clipboard_image(data: Vec<u8>, extension: Option<String>) -> Result<String, String> {
    if data.is_empty() {
        return Err("empty clipboard image data".into());
    }
    let ext = sanitize_extension(extension);
    let dir = std::env::temp_dir().join("ocr_to_doc");
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create temp dir: {e}"))?;
    let filename = format!("clipboard_{}.{}", Uuid::new_v4(), ext);
    let path = dir.join(filename);
    fs::write(&path, data).map_err(|e| format!("failed to write clipboard image: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn render_preview(
    path: String,
    page: Option<u32>,
    crop: Option<CropRect>,
    max_long_edge: Option<u32>,
    pdf_dpi: Option<u32>,
) -> Result<PreviewResponse, String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let python_bin = resolve_python_bin(&project_root);

    let helper = resolve_python_entry(&project_root, "ui_preview.py");
    if !helper.exists() {
        return Err(format!("ui_preview.py not found at {}", helper.display()));
    }

    let mut cmd = Command::new(&python_bin);
    apply_python_env(&mut cmd);
    cmd.arg("-u")
        .arg(helper)
        .arg("--input")
        .arg(&path)
        .arg("--page")
        .arg(page.unwrap_or(1).to_string());

    if let Some(c) = crop {
        cmd.arg("--crop").arg(format!(
            "{:.6},{:.6},{:.6},{:.6}",
            c.left, c.top, c.width, c.height
        ));
    }
    if let Some(max_le) = max_long_edge {
        cmd.arg("--max-long-edge").arg(max_le.to_string());
    }
    if let Some(dpi) = pdf_dpi {
        cmd.arg("--pdf-dpi").arg(dpi.to_string());
    }
    if let Some(poppler_bin) = resolve_poppler_bin_dir(&project_root) {
        cmd.arg("--poppler-path").arg(poppler_bin);
    }

    cmd.current_dir(&project_root);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run preview helper: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("preview helper failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    serde_json::from_str::<PreviewResponse>(&stdout)
        .map_err(|e| format!("failed to parse preview helper output: {e}"))
}

// resolve_python_entry, resolve_python_bin は paths.rs に移動済み

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
            current_message: job.current_message.clone(),
            page_current: job.page_current,
            page_total: job.page_total,
            eta_seconds: job.eta_seconds,
        });
    }
    Err("job not found".into())
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

        if let Some(index) = job.outputs.iter().position(|name| name == &filename) {
            if let Some(path) = job.output_paths.get(index) {
                let src = PathBuf::from(path);
                if src.exists() {
                    fs::copy(&src, &dest_path).map_err(|e| format!("failed to copy file: {e}"))?;
                    return Ok(());
                }
            }
        }

        // fallback: 元ファイルを探す
        let exe_dir =
            std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
        let project_root =
            resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
        let result_root = resolve_output_root_from_disk(&project_root);
        if let Some(src) = find_output_path(&result_root, &project_root, &filename) {
            fs::copy(&src, &dest_path).map_err(|e| format!("failed to copy file: {e}"))?;
            return Ok(());
        }
        return Err(format!("source file not found: {}", filename));
    }
    Err("job not found".into())
}

#[tauri::command]
fn open_output(
    job_id: String,
    filename: String,
    state: State<Arc<AppState>>,
) -> Result<(), String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;

    let job = jobs.get(&job_id).ok_or("job not found")?;
    // filename が outputs に含まれているか確認 (セキュリティ対策)
    if !job.outputs.contains(&filename) {
        return Err(format!("file not found in job outputs: {}", filename));
    }

    if let Some(index) = job.outputs.iter().position(|name| name == &filename) {
        if let Some(path) = job.output_paths.get(index) {
            let src = PathBuf::from(path);
            if src.exists() {
                return open_path_with_default_app(&src);
            }
        }
    }

    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let result_root = resolve_output_root_from_disk(&project_root);
    let src =
        find_output_path(&result_root, &project_root, &filename).ok_or("source file not found")?;
    open_path_with_default_app(&src)
}

#[tauri::command]
fn open_output_dir(job_id: String, state: State<Arc<AppState>>) -> Result<(), String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;

    let job = jobs.get(&job_id).ok_or("job not found")?;

    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let result_root = resolve_output_root_from_disk(&project_root);

    if let Some(first_path) = job.output_paths.first() {
        let src = PathBuf::from(first_path);
        if let Some(parent) = src.parent() {
            if parent.exists() {
                return open_path_with_default_app(parent);
            }
        }
    }

    if let Some(first) = job.outputs.first() {
        if let Some(src) = find_output_path(&result_root, &project_root, first) {
            if let Some(parent) = src.parent() {
                return open_path_with_default_app(parent);
            }
        }
    }

    // 出力が見つからない場合は result フォルダを開く
    open_path_with_default_app(&result_root)
}

#[tauri::command]
fn open_input_file(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is empty".into());
    }
    let p = PathBuf::from(trimmed);
    if !p.exists() {
        return Err(format!("file not found: {}", p.display()));
    }
    open_path_with_default_app(&p)
}

#[tauri::command]
fn open_readme() -> Result<(), String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

    let candidates = ["readme.md", "README.md"];
    for name in candidates {
        let path = project_root.join(name);
        if path.exists() {
            return open_path_with_default_app(&path);
        }
    }
    Err("README not found".into())
}

#[tauri::command]
fn list_recent_results(limit: Option<u32>) -> Result<Vec<RecentResultEntry>, String> {
    results::list_recent_results(limit)
}

#[tauri::command]
fn open_result_dir(dir_name: String) -> Result<(), String> {
    validate_result_dir_name(&dir_name)?;
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

    let result_root = resolve_output_root_from_disk(&project_root);
    let dir_path = result_root.join(&dir_name);
    if !dir_path.is_dir() {
        return Err("result dir not found".into());
    }

    let result_root_canon = canonicalize_dir(&result_root)?;
    let dir_canon = canonicalize_dir(&dir_path)?;
    if !dir_canon.starts_with(&result_root_canon) {
        return Err("invalid result dir".into());
    }

    open_path_with_default_app(&dir_canon)
}

#[tauri::command]
fn open_result_file(dir_name: String) -> Result<(), String> {
    validate_result_dir_name(&dir_name)?;
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

    let result_root = resolve_output_root_from_disk(&project_root);
    let dir_path = result_root.join(&dir_name);
    if !dir_path.is_dir() {
        return Err("result dir not found".into());
    }

    let result_root_canon = canonicalize_dir(&result_root)?;
    let dir_canon = canonicalize_dir(&dir_path)?;
    if !dir_canon.starts_with(&result_root_canon) {
        return Err("invalid result dir".into());
    }

    let best = pick_best_file_in_dir(&dir_canon, &dir_name).ok_or("no output file found")?;
    let file_path = dir_canon.join(&best);
    let file_canon = canonicalize_dir(&file_path)?;
    if !file_canon.starts_with(&dir_canon) {
        return Err("invalid output file".into());
    }
    open_path_with_default_app(&file_canon)
}

#[tauri::command]
fn get_pdf_page_count(path: String) -> Result<u32, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("ファイルが見つかりません: {path}"));
    }
    crate::ocr::pdf_to_images::pdf_page_count(&p, None)
}

/// PDF が埋め込みテキストを持つか（OCR スキップ候補になり得るか）を高速判定する。
/// ジョブ実行前のプレビュー用で、実際の抽出（extract_page_texts）はここでは行わない。
#[tauri::command]
async fn detect_pdf_text(path: String) -> Result<PdfTextDetectionResponse, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("ファイルが見つかりません: {path}"));
    }
    tokio::task::spawn_blocking(move || crate::ocr::pdf_text::classify_pdf(&p))
        .await
        .map_err(|e| format!("PDF 判定タスク失敗: {e}"))?
        .map(|c| PdfTextDetectionResponse {
            pdf_type: c.pdf_type.to_string(),
            confidence: c.confidence,
            eligible: c.eligible,
        })
}

#[tauri::command]
async fn check_environment() -> Result<EnvironmentStatus, String> {
    environment::check_environment().await
}

/// 設定画面のモデル選択用。指定エンジン / 接続先から利用可能なモデル名を取得する。
#[tauri::command]
async fn list_ocr_models(
    engine: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let cfg = crate::ollama::engine::BackendConfig::new(
        crate::ollama::engine::OcrEngine::parse(engine.as_deref()),
        base_url,
        api_key,
    );
    crate::ollama::engine::OcrBackend::new(&cfg).list_models().await
}

#[tauri::command]
fn load_settings() -> Result<AppSettings, String> {
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&exe_dir).unwrap_or_else(|| PathBuf::from("."));
    load_settings_from_disk(&project_root)
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<(), String> {
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&exe_dir).unwrap_or_else(|| PathBuf::from("."));
    let config_dir = resolve_config_dir(&project_root);
    settings::save_settings_to_disk(&settings, &config_dir)
}

/// アプリ終了時に OCR モデルを解放する。
/// Ollama 経路のときだけ実行し、llama.cpp（ユーザー管理のサーバー）には触れない。
async fn unload_model_if_ollama() {
    let Some(root) = std::env::current_exe()
        .ok()
        .and_then(|d| resolve_project_root(&d))
    else {
        return;
    };
    let Ok(s) = load_settings_from_disk(&root) else {
        return;
    };
    if crate::ollama::engine::OcrEngine::parse(s.ocr_engine.as_deref())
        != crate::ollama::engine::OcrEngine::Ollama
    {
        return;
    }
    let model = crate::ollama::engine::resolve_ocr_model(s.ocr_model);
    let _ = crate::ollama::client::OllamaClient::new()
        .unload_model(&model)
        .await;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            run_job_ollama,
            save_clipboard_image,
            render_preview,
            get_progress,
            get_result,
            save_file,
            open_output,
            open_output_dir,
            open_input_file,
            open_readme,
            list_recent_results,
            open_result_dir,
            open_result_file,
            check_environment,
            list_ocr_models,
            load_settings,
            save_settings,
            get_pdf_page_count,
            detect_pdf_text
        ])
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    unload_model_if_ollama().await;
                    app.exit(0);
                });
            }
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
            if let Some(project_root) = resolve_project_root(&exe_dir) {
                apply_window_settings(app.handle(), &project_root);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
